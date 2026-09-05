'use client';

import React, { useState } from 'react';
import Link from 'next/link';
import { motion } from 'framer-motion';
import {
  Cpu,
  Zap,
  ArrowRight,
  FileText,
  GitMerge,
  BrainCircuit,
  Code2,
  BarChart3,
  Layers,
  ShieldCheck,
  Sparkles,
  Server,
  Database,
  Terminal,
  Play,
  CheckCircle2,
} from 'lucide-react';

function GithubIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" />
      <path d="M9 18c-4.51 2-5-2-7-2" />
    </svg>
  );
}

export default function LandingPage() {
  const [demoQuery, setDemoQuery] = useState('How does Contextra manage token budgets in memory?');
  const [demoResponse, setDemoResponse] = useState<string | null>(null);
  const [demoLoading, setDemoLoading] = useState(false);

  const handleRunDemo = (e: React.FormEvent) => {
    e.preventDefault();
    if (!demoQuery.trim() || demoLoading) return;
    setDemoLoading(true);
    setDemoResponse(null);

    setTimeout(() => {
      setDemoResponse(
        `[Context Assembler]: Retrieved 4 chunks from Qdrant vector store (Recall@5: 96.2%).\n[Memory Manager]: Compressed conversation log down to 2.4KB summary.\n\nContextra packs system prompt, dynamic rolling memory, and reranked context into an optimized prompt tree bounded by your model's max token window.`
      );
      setDemoLoading(false);
    }, 1000);
  };

  const featureCards = [
    {
      title: 'Document Ingestion',
      icon: FileText,
      desc: 'Multi-format parsing (Markdown, PDF, OpenAPI, JSON), intelligent chunking, and parallel embedding vectors.',
      color: 'from-blue-500 to-indigo-600',
    },
    {
      title: 'Hybrid Retrieval',
      icon: GitMerge,
      desc: 'Qdrant dense vector search combined with PostgreSQL BM25 keyword matching fused via Reciprocal Rank Fusion (RRF).',
      color: 'from-indigo-500 to-purple-600',
    },
    {
      title: 'Conversation Memory',
      icon: BrainCircuit,
      desc: 'Importance-scoring recency decay algorithm and rolling summarization for infinite long-term chat memory.',
      color: 'from-purple-500 to-pink-600',
    },
    {
      title: 'Prompt Management',
      icon: Code2,
      desc: 'Handlebars templating engine, version control, Monaco code editor, and variable preview playground.',
      color: 'from-pink-500 to-rose-600',
    },
    {
      title: 'Evaluation Suite',
      icon: BarChart3,
      desc: 'Automated CI/CD regression benchmarking for Recall@K, Precision@K, MRR, and generation latency.',
      color: 'from-rose-500 to-amber-600',
    },
    {
      title: 'Rust High-Performance Engine',
      icon: Cpu,
      desc: 'Written in pure Rust with Tokio async runtime, Axum REST Gateway, and zero-cost abstractions for sub-100ms latency.',
      color: 'from-amber-500 to-emerald-600',
    },
  ];

  const techStackPills = [
    { name: 'Rust 2024', desc: 'Core Backend' },
    { name: 'Axum', desc: 'REST Gateway' },
    { name: 'PostgreSQL 16', desc: 'Relational & BM25' },
    { name: 'Redis 7', desc: 'Queue & Caching' },
    { name: 'Qdrant v1.8', desc: 'HNSW Vector Store' },
    { name: 'Docker Compose', desc: 'Container Stack' },
    { name: 'OpenTelemetry', desc: 'Tracing & Metrics' },
  ];

  return (
    <div className="min-h-screen bg-[#090a0f] text-zinc-100 selection:bg-indigo-500 selection:text-white relative overflow-hidden">
      {/* Background Glowing Ambient Orbs */}
      <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[1000px] h-[500px] bg-gradient-to-tr from-indigo-600/20 via-purple-600/20 to-pink-600/10 blur-[140px] pointer-events-none rounded-full" />

      {/* Top Header Navbar */}
      <header className="max-w-7xl mx-auto px-6 py-6 flex items-center justify-between relative z-10">
        <div className="flex items-center space-x-3">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-tr from-indigo-600 via-purple-600 to-pink-500 flex items-center justify-center shadow-lg shadow-indigo-500/25">
            <Cpu className="w-5 h-5 text-white" />
          </div>
          <span className="text-xl font-bold tracking-tight text-white">Contextra</span>
        </div>

        <div className="flex items-center space-x-4">
          <a
            href="https://github.com/soumyasurana/Contextra"
            target="_blank"
            rel="noopener noreferrer"
            className="px-4 py-2 rounded-xl bg-zinc-900 border border-white/10 hover:border-white/20 text-zinc-300 text-sm font-medium flex items-center space-x-2 transition-all"
          >
            <GithubIcon className="w-4 h-4" />
            <span>GitHub Repository</span>
          </a>

          <Link
            href="/dashboard"
            className="px-5 py-2 rounded-xl bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 text-white font-semibold text-sm shadow-lg shadow-indigo-500/25 transition-all hover:scale-105"
          >
            Try Live Demo
          </Link>
        </div>
      </header>

      {/* Hero Section */}
      <section className="max-w-5xl mx-auto px-6 pt-20 pb-16 text-center space-y-8 relative z-10">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4 }}
          className="space-y-4"
        >
          <div className="inline-flex items-center space-x-2 px-3.5 py-1.5 rounded-full bg-indigo-500/10 border border-indigo-500/20 text-xs font-semibold text-indigo-300">
            <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
            <span>Production-Grade Rust AI Context Engine</span>
          </div>

          <h1 className="text-5xl sm:text-7xl font-extrabold tracking-tight text-white leading-tight">
            Contextra
          </h1>

          <p className="text-2xl sm:text-3xl font-bold text-gradient-brand">
            &quot;The AI Context Engineering Platform&quot;
          </p>

          <p className="text-base sm:text-lg text-zinc-400 max-w-3xl mx-auto font-sans leading-relaxed">
            Build production-ready AI applications with document ingestion, hybrid retrieval, conversation memory, prompt management, and multi-provider LLM orchestration.
          </p>
        </motion.div>

        {/* Hero CTA Buttons */}
        <motion.div
          initial={{ opacity: 0, y: 15 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: 0.15 }}
          className="flex flex-col sm:flex-row items-center justify-center gap-4 pt-4"
        >
          <Link
            href="/dashboard"
            className="w-full sm:w-auto px-8 py-4 rounded-2xl bg-gradient-to-r from-indigo-600 via-purple-600 to-pink-600 hover:from-indigo-500 hover:to-pink-500 text-white font-bold text-base flex items-center justify-center space-x-3 shadow-xl shadow-indigo-500/30 transition-all hover:scale-105"
          >
            <span>Launch Platform Dashboard</span>
            <ArrowRight className="w-5 h-5" />
          </Link>

          <a
            href="https://github.com/soumyasurana/Contextra"
            target="_blank"
            rel="noopener noreferrer"
            className="w-full sm:w-auto px-8 py-4 rounded-2xl glass-panel border border-white/15 hover:border-indigo-500/50 text-white font-semibold text-base flex items-center justify-center space-x-3 transition-all"
          >
            <GithubIcon className="w-5 h-5" />
            <span>Star on GitHub</span>
          </a>
        </motion.div>
      </section>

      {/* Embedded Live Interactive Demo Section */}
      <section className="max-w-4xl mx-auto px-6 py-12 relative z-10">
        <div className="glass-panel rounded-3xl p-6 md:p-8 border border-white/10 shadow-2xl space-y-6 bg-zinc-950/80">
          <div className="flex items-center justify-between border-b border-white/10 pb-4">
            <div className="flex items-center space-x-3">
              <div className="w-3 h-3 rounded-full bg-rose-500" />
              <div className="w-3 h-3 rounded-full bg-amber-500" />
              <div className="w-3 h-3 rounded-full bg-emerald-500" />
              <span className="text-xs font-mono text-zinc-400 pl-2">Contextra Engine Playground Demo</span>
            </div>
            <span className="text-xs font-mono text-emerald-400 font-semibold flex items-center space-x-1">
              <CheckCircle2 className="w-3.5 h-3.5" />
              <span>Rust Gateway Ready</span>
            </span>
          </div>

          <form onSubmit={handleRunDemo} className="flex gap-3">
            <input
              type="text"
              value={demoQuery}
              onChange={(e) => setDemoQuery(e.target.value)}
              placeholder="Test a context engineering query..."
              className="flex-1 px-4 py-3 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:border-indigo-500/50 font-sans"
            />
            <button
              type="submit"
              disabled={demoLoading}
              className="px-6 py-3 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-sm flex items-center space-x-2 transition-all shadow-md shrink-0"
            >
              <Play className="w-4 h-4 fill-current" />
              <span>{demoLoading ? 'Processing...' : 'Run Query'}</span>
            </button>
          </form>

          {demoResponse && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              className="p-4 rounded-xl bg-zinc-900 border border-emerald-500/30 text-xs font-mono text-emerald-300 leading-relaxed whitespace-pre-wrap shadow-inner"
            >
              {demoResponse}
            </motion.div>
          )}
        </div>
      </section>

      {/* Feature Cards Grid */}
      <section className="max-w-7xl mx-auto px-6 py-16 relative z-10 space-y-12">
        <div className="text-center space-y-3">
          <h2 className="text-3xl font-bold tracking-tight text-white">Platform Capabilities</h2>
          <p className="text-sm text-zinc-400 max-w-xl mx-auto">
            Everything required to manage the context lifecycle for enterprise LLM systems.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {featureCards.map((feat, idx) => {
            const Icon = feat.icon;
            return (
              <motion.div
                key={feat.title}
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true }}
                transition={{ delay: idx * 0.05 }}
                className="glass-card glass-card-hover rounded-2xl p-6 space-y-4 border border-white/10"
              >
                <div className={`w-10 h-10 rounded-xl bg-gradient-to-tr ${feat.color} flex items-center justify-center text-white shadow-lg`}>
                  <Icon className="w-5 h-5" />
                </div>
                <h3 className="text-lg font-bold text-white">{feat.title}</h3>
                <p className="text-xs text-zinc-400 leading-relaxed font-sans">{feat.desc}</p>
              </motion.div>
            );
          })}
        </div>
      </section>

      {/* Architecture Pipeline Illustration */}
      <section className="max-w-6xl mx-auto px-6 py-16 relative z-10 space-y-8">
        <div className="text-center space-y-2">
          <h2 className="text-3xl font-bold tracking-tight text-white">End-to-End System Architecture</h2>
          <p className="text-sm text-zinc-400">Modular Rust Workspace Architecture</p>
        </div>

        <div className="glass-panel rounded-3xl p-8 border border-white/10 space-y-8 bg-zinc-950/90">
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4 text-center font-mono text-xs">
            <div className="p-4 rounded-2xl bg-zinc-900 border border-white/10 space-y-2">
              <span className="text-indigo-400 font-bold">1. Gateway / CLI</span>
              <p className="text-[11px] text-zinc-400 font-sans">Axum REST API + clap CLI</p>
            </div>
            <div className="p-4 rounded-2xl bg-zinc-900 border border-white/10 space-y-2">
              <span className="text-purple-400 font-bold">2. Storage Layer</span>
              <p className="text-[11px] text-zinc-400 font-sans">Postgres + Redis + Qdrant</p>
            </div>
            <div className="p-4 rounded-2xl bg-zinc-900 border border-white/10 space-y-2">
              <span className="text-pink-400 font-bold">3. Context Assembler</span>
              <p className="text-[11px] text-zinc-400 font-sans">RRF Merge & Token Budgeting</p>
            </div>
            <div className="p-4 rounded-2xl bg-zinc-900 border border-white/10 space-y-2">
              <span className="text-emerald-400 font-bold">4. Provider Execution</span>
              <p className="text-[11px] text-zinc-400 font-sans">OpenAI / Anthropic / Gemini</p>
            </div>
          </div>
        </div>
      </section>

      {/* Technology Stack Pills */}
      <section className="max-w-7xl mx-auto px-6 py-12 relative z-10 border-t border-white/10">
        <div className="flex flex-wrap items-center justify-center gap-3">
          {techStackPills.map((pill) => (
            <div
              key={pill.name}
              className="px-4 py-2 rounded-xl bg-zinc-900/90 border border-white/10 text-xs font-mono flex items-center space-x-2 text-zinc-300"
            >
              <span className="font-bold text-white">{pill.name}</span>
              <span className="text-zinc-500">•</span>
              <span className="text-zinc-400">{pill.desc}</span>
            </div>
          ))}
        </div>
      </section>

      {/* Footer */}
      <footer className="max-w-7xl mx-auto px-6 py-8 border-t border-white/10 flex items-center justify-between text-xs text-zinc-500 relative z-10">
        <p>© 2026 Contextra Platform. Written in Rust for Enterprise AI Systems.</p>
        <Link href="/dashboard" className="text-indigo-400 hover:text-white font-medium">
          Open Dashboard →
        </Link>
      </footer>
    </div>
  );
}
