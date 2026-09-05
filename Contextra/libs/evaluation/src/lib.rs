use errors::ContextraError;
use providers::{ChatMessage, ChatRequest, LLMProvider};
use retrieval::{RetrievalRequest, Retriever};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const DEFAULT_JUDGE_MODEL: &str = "gpt-4.1-mini";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkDataset {
    #[serde(default)]
    pub retrieval: Vec<RetrievalEvaluationCase>,
    #[serde(default)]
    pub generation: Vec<GenerationEvaluationCase>,
}

impl BenchmarkDataset {
    pub fn from_json_str(source: &str) -> Result<Self, ContextraError> {
        serde_json::from_str(source).map_err(|error| {
            ContextraError::Validation(format!("failed to parse benchmark dataset: {error}"))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalEvaluationCase {
    pub query: String,
    pub expected_relevant_chunk_ids: Vec<String>,
    #[serde(default)]
    pub retrieved_chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalRunCase {
    pub query: String,
    pub collection: String,
    pub expected_relevant_chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationEvaluationCase {
    pub query: String,
    pub answer: String,
    pub reference_answer: String,
    #[serde(default = "default_min_words")]
    pub min_words: usize,
    #[serde(default = "default_max_words")]
    pub max_words: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RetrievalMetrics {
    pub precision_at_k: f32,
    pub recall_at_k: f32,
    pub mrr: f32,
}

impl RetrievalMetrics {
    pub fn average(metrics: &[Self]) -> Self {
        if metrics.is_empty() {
            return Self::default();
        }

        let count = metrics.len() as f32;
        Self {
            precision_at_k: metrics
                .iter()
                .map(|metric| metric.precision_at_k)
                .sum::<f32>()
                / count,
            recall_at_k: metrics.iter().map(|metric| metric.recall_at_k).sum::<f32>() / count,
            mrr: metrics.iter().map(|metric| metric.mrr).sum::<f32>() / count,
        }
    }
}

impl Default for RetrievalMetrics {
    fn default() -> Self {
        Self {
            precision_at_k: 0.0,
            recall_at_k: 0.0,
            mrr: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalCaseResult {
    pub query: String,
    pub expected_relevant_chunk_ids: Vec<String>,
    pub retrieved_chunk_ids: Vec<String>,
    pub metrics: RetrievalMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GenerationMetrics {
    pub judge_score: f32,
    pub answer_relevance: f32,
    pub length_score: f32,
    pub overall_score: f32,
}

impl GenerationMetrics {
    pub fn average(metrics: &[Self]) -> Self {
        if metrics.is_empty() {
            return Self::default();
        }

        let count = metrics.len() as f32;
        Self {
            judge_score: metrics.iter().map(|metric| metric.judge_score).sum::<f32>() / count,
            answer_relevance: metrics
                .iter()
                .map(|metric| metric.answer_relevance)
                .sum::<f32>()
                / count,
            length_score: metrics
                .iter()
                .map(|metric| metric.length_score)
                .sum::<f32>()
                / count,
            overall_score: metrics
                .iter()
                .map(|metric| metric.overall_score)
                .sum::<f32>()
                / count,
        }
    }
}

impl Default for GenerationMetrics {
    fn default() -> Self {
        Self {
            judge_score: 0.0,
            answer_relevance: 0.0,
            length_score: 0.0,
            overall_score: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationCaseResult {
    pub query: String,
    pub answer: String,
    pub reference_answer: String,
    pub metrics: GenerationMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub retrieval_cases: Vec<RetrievalCaseResult>,
    pub generation_cases: Vec<GenerationCaseResult>,
    pub retrieval_summary: RetrievalMetrics,
    pub generation_summary: GenerationMetrics,
}

#[derive(Debug, Clone)]
pub struct EvaluationPipeline<P> {
    judge_provider: P,
    judge_model: String,
}

impl<P> EvaluationPipeline<P> {
    pub fn new(judge_provider: P) -> Self {
        Self {
            judge_provider,
            judge_model: DEFAULT_JUDGE_MODEL.to_string(),
        }
    }

    pub fn with_judge_model(mut self, judge_model: impl Into<String>) -> Self {
        self.judge_model = judge_model.into();
        self
    }

    pub fn evaluate_retrieval_cases(
        &self,
        cases: &[RetrievalEvaluationCase],
        k: usize,
    ) -> Vec<RetrievalCaseResult> {
        cases
            .iter()
            .map(|case| RetrievalCaseResult {
                query: case.query.clone(),
                expected_relevant_chunk_ids: case.expected_relevant_chunk_ids.clone(),
                retrieved_chunk_ids: case.retrieved_chunk_ids.clone(),
                metrics: retrieval_metrics(
                    &case.expected_relevant_chunk_ids,
                    &case.retrieved_chunk_ids,
                    k,
                ),
            })
            .collect()
    }
}

impl<P> EvaluationPipeline<P>
where
    P: LLMProvider,
{
    pub async fn evaluate(
        &self,
        dataset: &BenchmarkDataset,
        k: usize,
    ) -> Result<EvaluationReport, ContextraError> {
        let retrieval_cases = self.evaluate_retrieval_cases(&dataset.retrieval, k);
        let generation_cases = self.evaluate_generation_cases(&dataset.generation).await?;
        Ok(report(retrieval_cases, generation_cases))
    }

    pub async fn evaluate_retriever<R>(
        &self,
        retriever: &R,
        cases: &[RetrievalRunCase],
        k: usize,
    ) -> Result<Vec<RetrievalCaseResult>, ContextraError>
    where
        R: Retriever,
    {
        let mut results = Vec::new();
        for case in cases {
            let retrieved = retriever
                .retrieve(RetrievalRequest::hybrid(
                    case.query.clone(),
                    case.collection.clone(),
                    k,
                ))
                .await?
                .into_iter()
                .map(|document| document.id.to_string())
                .collect::<Vec<_>>();

            results.push(RetrievalCaseResult {
                query: case.query.clone(),
                expected_relevant_chunk_ids: case.expected_relevant_chunk_ids.clone(),
                metrics: retrieval_metrics(&case.expected_relevant_chunk_ids, &retrieved, k),
                retrieved_chunk_ids: retrieved,
            });
        }
        Ok(results)
    }

    pub async fn evaluate_generation_cases(
        &self,
        cases: &[GenerationEvaluationCase],
    ) -> Result<Vec<GenerationCaseResult>, ContextraError> {
        let mut results = Vec::new();
        for case in cases {
            let judge_score = self.judge_generation(case).await?;
            let answer_relevance = answer_relevance(&case.query, &case.answer);
            let length_score = length_score(&case.answer, case.min_words, case.max_words);
            let overall_score =
                ((judge_score * 0.60) + (answer_relevance * 0.25) + (length_score * 0.15))
                    .clamp(0.0, 1.0);

            results.push(GenerationCaseResult {
                query: case.query.clone(),
                answer: case.answer.clone(),
                reference_answer: case.reference_answer.clone(),
                metrics: GenerationMetrics {
                    judge_score,
                    answer_relevance,
                    length_score,
                    overall_score,
                },
            });
        }
        Ok(results)
    }

    async fn judge_generation(
        &self,
        case: &GenerationEvaluationCase,
    ) -> Result<f32, ContextraError> {
        let prompt = format!(
            "Score the answer from 0.0 to 1.0 for correctness and usefulness.\n\
             Return only a number.\n\n\
             Query: {}\n\
             Reference answer: {}\n\
             Candidate answer: {}",
            case.query, case.reference_answer, case.answer
        );
        let response = self
            .judge_provider
            .chat(ChatRequest::new(
                self.judge_model.clone(),
                vec![
                    ChatMessage::system("You are a strict evaluation judge."),
                    ChatMessage::user(prompt),
                ],
            ))
            .await
            .map_err(ContextraError::from)?;

        let content = response.message.content.ok_or_else(|| {
            ContextraError::ProviderError("judge provider returned no content".to_string())
        })?;

        parse_score(&content).ok_or_else(|| {
            ContextraError::Validation(format!("judge score was not a number: {content}"))
        })
    }
}

pub fn report(
    retrieval_cases: Vec<RetrievalCaseResult>,
    generation_cases: Vec<GenerationCaseResult>,
) -> EvaluationReport {
    let retrieval_metrics = retrieval_cases
        .iter()
        .map(|case| case.metrics)
        .collect::<Vec<_>>();
    let generation_metrics = generation_cases
        .iter()
        .map(|case| case.metrics)
        .collect::<Vec<_>>();

    EvaluationReport {
        retrieval_cases,
        generation_cases,
        retrieval_summary: RetrievalMetrics::average(&retrieval_metrics),
        generation_summary: GenerationMetrics::average(&generation_metrics),
    }
}

pub fn retrieval_metrics(
    expected_relevant_chunk_ids: &[String],
    retrieved_chunk_ids: &[String],
    k: usize,
) -> RetrievalMetrics {
    if k == 0 || expected_relevant_chunk_ids.is_empty() {
        return RetrievalMetrics::default();
    }

    let expected = expected_relevant_chunk_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let top_k = retrieved_chunk_ids.iter().take(k).collect::<Vec<_>>();
    let relevant_in_top_k = top_k
        .iter()
        .filter(|chunk_id| expected.contains(chunk_id.as_str()))
        .count();
    let first_relevant_rank = retrieved_chunk_ids
        .iter()
        .take(k)
        .position(|chunk_id| expected.contains(chunk_id.as_str()))
        .map(|index| index + 1);

    RetrievalMetrics {
        precision_at_k: relevant_in_top_k as f32 / k as f32,
        recall_at_k: relevant_in_top_k as f32 / expected.len() as f32,
        mrr: first_relevant_rank
            .map(|rank| 1.0 / rank as f32)
            .unwrap_or(0.0),
    }
}

pub fn answer_relevance(query: &str, answer: &str) -> f32 {
    let query_terms = normalized_terms(query);
    if query_terms.is_empty() {
        return 0.0;
    }

    let answer_terms = normalized_terms(answer);
    let overlap = query_terms
        .iter()
        .filter(|term| answer_terms.contains(*term))
        .count();
    overlap as f32 / query_terms.len() as f32
}

pub fn length_score(answer: &str, min_words: usize, max_words: usize) -> f32 {
    let words = answer.split_whitespace().count();
    if words == 0 || max_words < min_words {
        return 0.0;
    }
    if (min_words..=max_words).contains(&words) {
        return 1.0;
    }
    if words < min_words {
        return words as f32 / min_words.max(1) as f32;
    }

    (max_words as f32 / words as f32).clamp(0.0, 1.0)
}

fn parse_score(content: &str) -> Option<f32> {
    content
        .split(|character: char| {
            !(character.is_ascii_digit() || character == '.' || character == '-')
        })
        .filter(|token| !token.is_empty() && *token != "." && *token != "-")
        .find_map(|token| token.parse::<f32>().ok())
        .map(|score| score.clamp(0.0, 1.0))
}

fn normalized_terms(text: &str) -> HashSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|term| term.len() > 2)
        .collect()
}

fn default_min_words() -> usize {
    1
}

fn default_max_words() -> usize {
    usize::MAX
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::stream;
    use providers::{ChatResponse, ChatStream, ProviderError};
    use std::sync::{Arc, Mutex};

    const FIXTURE: &str = include_str!("../../../tests/benchmarks/evaluation_fixture.json");

    #[test]
    fn retrieval_metrics_match_fixture_dataset() -> Result<(), Box<dyn std::error::Error>> {
        let dataset = BenchmarkDataset::from_json_str(FIXTURE)?;
        let pipeline = EvaluationPipeline::new(MockJudge::new(vec![]));

        let results = pipeline.evaluate_retrieval_cases(&dataset.retrieval, 3);
        let summary = RetrievalMetrics::average(
            &results
                .iter()
                .map(|result| result.metrics)
                .collect::<Vec<_>>(),
        );

        assert_close(results[0].metrics.precision_at_k, 2.0 / 3.0);
        assert_close(results[0].metrics.recall_at_k, 1.0);
        assert_close(results[0].metrics.mrr, 1.0);
        assert_close(results[1].metrics.precision_at_k, 1.0 / 3.0);
        assert_close(results[1].metrics.recall_at_k, 1.0);
        assert_close(results[1].metrics.mrr, 0.5);
        assert_close(results[2].metrics.precision_at_k, 0.0);
        assert_close(results[2].metrics.recall_at_k, 0.0);
        assert_close(results[2].metrics.mrr, 0.0);
        assert_close(summary.precision_at_k, 1.0 / 3.0);
        assert_close(summary.recall_at_k, 2.0 / 3.0);
        assert_close(summary.mrr, 0.5);
        Ok(())
    }

    #[tokio::test]
    async fn evaluation_report_combines_retrieval_generation_and_judge_scores()
    -> Result<(), Box<dyn std::error::Error>> {
        let dataset = BenchmarkDataset::from_json_str(FIXTURE)?;
        let judge = MockJudge::new(vec!["0.8", "score: 0.6"]);
        let pipeline = EvaluationPipeline::new(judge.clone()).with_judge_model("judge-model");

        let report = pipeline.evaluate(&dataset, 3).await?;

        assert_eq!(report.retrieval_cases.len(), 3);
        assert_eq!(report.generation_cases.len(), 2);
        assert_close(report.retrieval_summary.precision_at_k, 1.0 / 3.0);
        assert_close(report.generation_cases[0].metrics.judge_score, 0.8);
        assert_close(report.generation_cases[1].metrics.judge_score, 0.6);
        assert!(report.generation_summary.overall_score > 0.0);

        let requests = judge.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].model, "judge-model");
        assert!(
            requests[0]
                .messages
                .iter()
                .any(|message| message.content.as_deref().is_some_and(|content| {
                    content.contains("Reference answer") && content.contains("Candidate answer")
                }))
        );
        Ok(())
    }

    #[test]
    fn heuristic_generation_checks_score_relevance_and_length() {
        assert_close(
            answer_relevance("context engine ranking", "The context engine uses signals."),
            2.0 / 3.0,
        );
        assert_close(length_score("one two three", 2, 4), 1.0);
        assert_close(length_score("one two", 4, 8), 0.5);
    }

    #[derive(Debug, Clone)]
    struct MockJudge {
        scores: Arc<Mutex<Vec<String>>>,
        requests: Arc<Mutex<Vec<ChatRequest>>>,
    }

    impl MockJudge {
        fn new(scores: Vec<&str>) -> Self {
            Self {
                scores: Arc::new(Mutex::new(
                    scores.into_iter().map(ToString::to_string).collect(),
                )),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl LLMProvider for MockJudge {
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
            self.requests.lock().unwrap().push(request.clone());
            let score = self
                .scores
                .lock()
                .unwrap()
                .first()
                .cloned()
                .unwrap_or_else(|| "0.5".to_string());
            if !self.scores.lock().unwrap().is_empty() {
                self.scores.lock().unwrap().remove(0);
            }

            Ok(ChatResponse {
                id: "judge-response".to_string(),
                model: request.model,
                message: ChatMessage::assistant(score),
                finish_reason: Some("stop".to_string()),
                usage: None,
            })
        }

        async fn chat_stream(&self, _request: ChatRequest) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(stream::empty()))
        }

        fn supports_function_calling(&self) -> bool {
            false
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "actual={actual}, expected={expected}"
        );
    }
}
