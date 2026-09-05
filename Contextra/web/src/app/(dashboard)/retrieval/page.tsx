'use client';

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  GitMerge,
  Search,
  Cpu,
  Layers,
  Sparkles,
  Zap,
  ArrowRight,
  Database,
  CheckCircle2,
  Sliders,
  FileText,
} from 'lucide-react';
import { RetrievalStepInfo, RetrievedChunk } from '@/types';
import { MOCK_RETRIEVED_CHUNKS } from '@/lib/mock-data';

export default function RetrievalPage() {
  const [testQuery, setTestQuery] = useState('How does Reciprocal Rank Fusion calculate vector scores?');
  const [isRunning, setIsRunning] = useState(false);
  const [activeStep, setActiveStep] = useState<number>(4); // default finished step

  const steps: RetrievalStepInfo[] = [
    {
      id: 'step_1',
      name: 'Query Embedding',
      description: 'Generates 1536-dim vector representation using text-embedding-3-small',
      latency_ms: 18,
      input_count: 1,
      output_count: 1,
      status: 'completed',
      details: [
        { label: 'Embedding Model', value: 'text-embedding-3-small' },
        { label: 'Dimensions', value: 1536 },
        { label: 'Normalization', value: 'L2 Normalized' },
      ],
    },
    {
      id: 'step_2',
      name: 'Vector Search (Qdrant)',
      description: 'HNSW graph traversal over 18,450 vectors looking for cosine nearest neighbors',
      latency_ms: 24,
      input_count: 1,
      output_count: 25,
      status: 'completed',
      details: [
        { label: 'Vector Store', value: 'Qdrant Cluster' },
        { label: 'Metric', value: 'Cosine Similarity' },
        { label: 'Top-K Candidates', value: 25 },
      ],
    },
    {
      id: 'step_3',
      name: 'Keyword Search (BM25)',
      description: 'Inverted index term frequency match over PostgreSQL payload text',
      latency_ms: 12,
      input_count: 1,
      output_count: 25,
      status: 'completed',
      details: [
        { label: 'Engine', value: 'Postgres / Redis' },
        { label: 'Algorithm', value: 'BM25 (k1=1.2, b=0.75)' },
        { label: 'Matches', value: 25 },
      ],
    },
    {
      id: 'step_4',
      name: 'Hybrid Merge (RRF)',
      description: 'Combines vector and keyword ranks using Reciprocal Rank Fusion (k=60)',
      latency_ms: 8,
      input_count: 50,
      output_count: 15,
      status: 'completed',
      details: [
        { label: 'Fusion Method', value: 'Reciprocal Rank Fusion' },
        { label: 'RRF Constant (k)', value: 60 },
        { label: 'Merged Candidates', value: 15 },
      ],
    },
    {
      id: 'step_5',
      name: 'Reranker (Cohere)',
      description: 'Cross-encoder transformer rescores merged chunks against full query semantics',
      latency_ms: 45,
      input_count: 15,
      output_count: 5,
      status: 'completed',
      details: [
        { label: 'Reranker Model', value: 'bge-reranker-large' },
        { label: 'Final Top K', value: 5 },
        { label: 'Score Delta', value: '+14.2%' },
      ],
    },
  ];

  const handleRunPipeline = (e: React.FormEvent) => {
    e.preventDefault();
    if (!testQuery.trim() || isRunning) return;

    setIsRunning(true);
    setActiveStep(0);

    const interval = setInterval(() => {
      setActiveStep((prev) => {
        if (prev >= steps.length - 1) {
          clearInterval(interval);
          setIsRunning(false);
          return steps.length - 1;
        }
        return prev + 1;
      });
    }, 400);
  };

  return (
    <div className="space-y-8">
      {/* Page Header */}
      <div>
        <h1 className="text-2xl font-bold tracking-tight text-white flex items-center space-x-2">
          <GitMerge className="w-6 h-6 text-indigo-400" />
          <span>Retrieval Pipeline Visualizer</span>
        </h1>
        <p className="text-sm text-zinc-400 mt-1">
          Inspect multi-stage hybrid retrieval execution: Dense Vector Search ➔ BM25 Keyword ➔ RRF Merging ➔ Cross-Encoder Reranking.
        </p>
      </div>

      {/* Interactive Query Input */}
      <form onSubmit={handleRunPipeline} className="glass-panel p-4 rounded-2xl flex flex-col md:flex-row items-center gap-3">
        <div className="relative flex-1 w-full">
          <Search className="w-4 h-4 text-zinc-400 absolute left-3 top-3.5" />
          <input
            type="text"
            value={testQuery}
            onChange={(e) => setTestQuery(e.target.value)}
            placeholder="Type a test query to trace through the pipeline..."
            className="w-full pl-9 pr-4 py-2.5 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:border-indigo-500/50 font-sans"
          />
        </div>
        <button
          type="submit"
          disabled={isRunning}
          className="w-full md:w-auto px-5 py-2.5 rounded-xl bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 text-white font-medium text-sm flex items-center justify-center space-x-2 shadow-lg shadow-indigo-500/20 transition-all shrink-0"
        >
          <Zap className={`w-4 h-4 ${isRunning ? 'animate-bounce text-amber-400' : ''}`} />
          <span>{isRunning ? 'Executing Steps...' : 'Execute Pipeline Trace'}</span>
        </button>
      </form>

      {/* Pipeline Flow Stepper Cards */}
      <div className="grid grid-cols-1 md:grid-cols-5 gap-3">
        {steps.map((step, idx) => {
          const isActive = idx === activeStep;
          const isDone = idx <= activeStep;
          return (
            <motion.div
              key={step.id}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: idx * 0.05 }}
              className={`glass-card p-4 rounded-2xl border flex flex-col justify-between space-y-3 relative transition-all ${
                isActive
                  ? 'border-indigo-500 bg-indigo-600/15 shadow-xl shadow-indigo-500/20 scale-[1.02]'
                  : isDone
                  ? 'border-emerald-500/30 bg-zinc-900/60'
                  : 'border-white/5 opacity-50'
              }`}
            >
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-mono uppercase font-bold text-zinc-400">Step 0{idx + 1}</span>
                {isDone && <CheckCircle2 className="w-4 h-4 text-emerald-400" />}
              </div>

              <div>
                <h3 className="text-xs font-bold text-white">{step.name}</h3>
                <p className="text-[10px] text-zinc-400 mt-1 line-clamp-2">{step.description}</p>
              </div>

              <div className="pt-2 border-t border-white/5 flex items-center justify-between text-[10px] font-mono text-zinc-400">
                <span>Latency</span>
                <span className="text-emerald-400 font-bold">{step.latency_ms} ms</span>
              </div>
            </motion.div>
          );
        })}
      </div>

      {/* Deep-Dive Active Step Details */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Step Telemetry */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <div className="flex items-center justify-between border-b border-white/10 pb-3">
            <h3 className="text-sm font-bold text-white flex items-center space-x-2">
              <Sliders className="w-4 h-4 text-indigo-400" />
              <span>Step Execution Telemetry</span>
            </h3>
            <span className="text-xs font-mono text-indigo-400">{steps[activeStep]?.name}</span>
          </div>

          <div className="space-y-3">
            {steps[activeStep]?.details.map((d) => (
              <div key={d.label} className="flex items-center justify-between p-2.5 rounded-xl bg-zinc-900/80 border border-white/5 text-xs">
                <span className="text-zinc-400">{d.label}</span>
                <span className="font-mono text-zinc-100 font-medium">{d.value}</span>
              </div>
            ))}
          </div>

          <div className="p-3 rounded-xl bg-zinc-950 border border-white/10 text-xs space-y-1 font-mono">
            <div className="flex justify-between text-zinc-400">
              <span>Input Candidates</span>
              <span className="text-zinc-200">{steps[activeStep]?.input_count}</span>
            </div>
            <div className="flex justify-between text-zinc-400">
              <span>Output Candidates</span>
              <span className="text-emerald-400">{steps[activeStep]?.output_count}</span>
            </div>
          </div>
        </div>

        {/* Final Retrieved Context Results (Spans 2 columns) */}
        <div className="lg:col-span-2 glass-panel rounded-2xl p-6 space-y-4">
          <div className="flex items-center justify-between border-b border-white/10 pb-3">
            <h3 className="text-sm font-bold text-white flex items-center space-x-2">
              <FileText className="w-4 h-4 text-emerald-400" />
              <span>Final Reranked Context Chunks</span>
            </h3>
            <span className="text-xs font-mono text-zinc-400">5 Top Chunks</span>
          </div>

          <div className="space-y-3">
            {MOCK_RETRIEVED_CHUNKS.map((chunk) => (
              <div key={chunk.id} className="p-4 rounded-xl glass-card border border-white/10 space-y-2 text-xs">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-white truncate max-w-sm">{chunk.document_name}</span>
                  <span className="font-mono text-xs text-emerald-400 font-bold bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
                    RRF Score: {(chunk.score * 100).toFixed(1)}%
                  </span>
                </div>
                <p className="text-zinc-300 font-mono text-xs bg-zinc-950/80 p-3 rounded-lg border border-white/5 leading-relaxed">
                  {chunk.content}
                </p>
                <div className="flex items-center justify-between text-[11px] text-zinc-500 pt-1">
                  <span>Collection: {chunk.collection_name}</span>
                  <span>Chunk Offset #{chunk.chunk_index}</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
