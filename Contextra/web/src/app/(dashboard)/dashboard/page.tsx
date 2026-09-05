'use client';

import React, { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import {
  FileText,
  Layers,
  FolderArchive,
  MessageSquare,
  Code2,
  Cpu,
  Activity,
  Clock,
  TrendingUp,
  Server,
  Zap,
  CheckCircle2,
  XCircle,
  RefreshCw,
  AlertTriangle,
} from 'lucide-react';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  BarChart,
  Bar,
  PieChart,
  Pie,
  Cell,
} from 'recharts';
import { useAppStore } from '@/lib/store';
import { GATEWAY_URL } from '@/lib/api';

export default function DashboardPage() {
  const {
    documentsCount,
    collectionsCount,
    conversationsCount,
    documents,
    collections,
    conversations,
    prompts,
    apiConnected,
    fetchDashboardData,
  } = useAppStore();

  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let isMounted = true;
    fetchDashboardData().finally(() => {
      if (isMounted) setIsLoading(false);
    });
    return () => {
      isMounted = false;
    };
  }, [fetchDashboardData]);

  // Compute stats from real backend data
  const totalChunks = documents.reduce((acc, d) => acc + (d.chunks_count || 0), 0);
  const totalEmbeddings = totalChunks; // 1-to-1 vector embedding per chunk

  const metricCards = [
    { label: 'Total Documents', value: documentsCount.toLocaleString(), icon: FileText, change: 'Live', color: 'from-blue-500 to-indigo-600' },
    { label: 'Ingested Chunks', value: totalChunks.toLocaleString(), icon: Layers, change: 'Live', color: 'from-indigo-500 to-purple-600' },
    { label: 'Collections', value: collectionsCount.toLocaleString(), icon: FolderArchive, change: 'Live', color: 'from-purple-500 to-pink-600' },
    { label: 'Conversations', value: conversationsCount.toLocaleString(), icon: MessageSquare, change: 'Live', color: 'from-emerald-500 to-teal-600' },
    { label: 'Prompt Templates', value: prompts.length.toLocaleString(), icon: Code2, change: 'Local', color: 'from-amber-500 to-orange-600' },
    { label: 'Vector Embeddings', value: totalEmbeddings.toLocaleString(), icon: Cpu, change: 'Live', color: 'from-cyan-500 to-blue-600' },
    { label: 'Total Requests', value: '0', icon: Activity, change: 'Telemetry off', color: 'from-pink-500 to-rose-600' },
    { label: 'Avg Latency', value: '—', icon: Clock, change: 'Telemetry off', color: 'from-emerald-400 to-green-600' },
  ];

  return (
    <div className="space-y-8">
      {/* Page Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight text-white flex items-center space-x-3">
            <span>Platform Overview</span>
            <span
              className={`px-2.5 py-0.5 text-xs font-semibold border rounded-full flex items-center space-x-1.5 ${
                apiConnected
                  ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                  : 'bg-rose-500/10 text-rose-400 border-rose-500/20'
              }`}
            >
              <span
                className={`w-2 h-2 rounded-full ${
                  apiConnected ? 'bg-emerald-400 animate-pulse' : 'bg-rose-400'
                }`}
              />
              <span>{apiConnected ? 'Backend Connected' : 'Backend Disconnected'}</span>
            </span>
          </h1>
          <p className="text-sm text-zinc-400 mt-1">
            Real-time context engineering telemetry, vector indexing, and pipeline performance metrics.
          </p>
        </div>

        <div className="flex items-center space-x-3">
          <button
            onClick={() => {
              setIsLoading(true);
              fetchDashboardData().finally(() => setIsLoading(false));
            }}
            className="px-3 py-1.5 rounded-xl glass-card text-xs text-zinc-300 hover:text-white flex items-center space-x-2 transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isLoading ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>
          <div className="px-3 py-1.5 rounded-xl glass-card text-xs text-zinc-300 flex items-center space-x-2">
            <Zap className="w-3.5 h-3.5 text-amber-400" />
            <span>Qdrant Vectors: <strong className="text-white font-mono">{totalEmbeddings.toLocaleString()}</strong></span>
          </div>
        </div>
      </div>

      {/* Connection Warning Banner if backend is not reachable */}
      {!apiConnected && !isLoading && (
        <div className="p-4 rounded-2xl bg-rose-500/10 border border-rose-500/20 flex items-center space-x-3 text-xs text-rose-300">
          <AlertTriangle className="w-5 h-5 text-rose-400 shrink-0" />
          <div>
            <p className="font-semibold">Unable to connect to Contextra Backend Gateway {GATEWAY_URL}</p>
            <p className="text-rose-400/80 mt-0.5">
              Make sure the Rust gateway service is running. Data below represents local state or 0 values until connected.
            </p>
          </div>
        </div>
      )}

      {/* Metrics Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {metricCards.map((card, idx) => {
          const Icon = card.icon;
          return (
            <motion.div
              key={card.label}
              initial={{ opacity: 0, y: 15 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.25, delay: idx * 0.04 }}
              className="glass-card glass-card-hover rounded-2xl p-5 relative overflow-hidden"
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-zinc-400">{card.label}</span>
                <div className={`w-8 h-8 rounded-xl bg-gradient-to-tr ${card.color} flex items-center justify-center text-white shadow-md`}>
                  <Icon className="w-4 h-4" />
                </div>
              </div>

              <div className="mt-3 flex items-baseline justify-between">
                <span className="text-2xl font-bold tracking-tight text-white font-mono">
                  {isLoading ? '...' : card.value}
                </span>
                <span className="text-xs font-semibold text-zinc-500 flex items-center space-x-0.5">
                  <span>{card.change}</span>
                </span>
              </div>
            </motion.div>
          );
        })}
      </div>

      {/* Main Charts Section */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Requests & Latency Area Chart */}
        <div className="lg:col-span-2 glass-panel rounded-2xl p-6 space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-base font-semibold text-white">Requests & Latency Over Time</h2>
              <p className="text-xs text-zinc-400">24-hour API request volume and average response time (ms)</p>
            </div>
          </div>

          <div className="h-72 w-full pt-4 flex flex-col items-center justify-center border border-white/5 rounded-xl bg-zinc-950/40 text-center">
            <Activity className="w-8 h-8 text-zinc-600 mb-2" />
            <p className="text-xs font-medium text-zinc-400">No request telemetry data recorded yet</p>
            <p className="text-[11px] text-zinc-600 mt-1 max-w-xs">
              Execute queries in the RAG playground to generate real-time request metrics.
            </p>
          </div>
        </div>

        {/* Provider Usage Distribution */}
        <div className="glass-panel rounded-2xl p-6 space-y-4 flex flex-col justify-between">
          <div>
            <h2 className="text-base font-semibold text-white">LLM Provider Usage</h2>
            <p className="text-xs text-zinc-400">Distribution of chat completions across model providers</p>
          </div>

          <div className="h-52 w-full flex flex-col items-center justify-center border border-white/5 rounded-xl bg-zinc-950/40 text-center">
            <Cpu className="w-8 h-8 text-zinc-600 mb-2" />
            <p className="text-xs font-medium text-zinc-400">No model executions</p>
            <p className="text-[11px] text-zinc-600 mt-1">Provider telemetry will display here after chat sessions.</p>
          </div>
        </div>
      </div>

      {/* Second Row: Latency Breakdown & System Status */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Latency Pipeline Breakdown */}
        <div className="lg:col-span-2 glass-panel rounded-2xl p-6 space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-base font-semibold text-white">Retrieval Pipeline Latency (p95 ms)</h2>
              <p className="text-xs text-zinc-400">Time spent in each sub-system during context retrieval</p>
            </div>
          </div>

          <div className="h-60 w-full flex flex-col items-center justify-center border border-white/5 rounded-xl bg-zinc-950/40 text-center">
            <Clock className="w-8 h-8 text-zinc-600 mb-2" />
            <p className="text-xs font-medium text-zinc-400">No retrieval latency benchmarks collected</p>
          </div>
        </div>

        {/* System Health & Status */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <div className="flex items-center justify-between border-b border-white/10 pb-3">
            <h2 className="text-base font-semibold text-white flex items-center space-x-2">
              <Server className="w-4 h-4 text-indigo-400" />
              <span>System Status</span>
            </h2>
            <span className={`text-xs font-semibold ${apiConnected ? 'text-emerald-400' : 'text-rose-400'}`}>
              {apiConnected ? 'Gateway Online' : 'Gateway Offline'}
            </span>
          </div>

          <div className="space-y-3">
            <div className="flex items-center justify-between p-2.5 rounded-xl bg-zinc-900/60 border border-white/5 text-xs">
              <div className="flex items-center space-x-2.5">
                {apiConnected ? (
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                ) : (
                  <XCircle className="w-4 h-4 text-rose-400 shrink-0" />
                )}
                <div>
                  <p className="text-zinc-200 font-medium">Rust Gateway API</p>
                  <p className="text-[10px] text-zinc-500 font-mono">{GATEWAY_URL}</p>
                </div>
              </div>
              <div className="text-right font-mono">
                <p className={apiConnected ? 'text-emerald-400 font-medium' : 'text-rose-400 font-medium'}>
                  {apiConnected ? 'Healthy' : 'Unreachable'}
                </p>
              </div>
            </div>

            <div className="flex items-center justify-between p-2.5 rounded-xl bg-zinc-900/60 border border-white/5 text-xs">
              <div className="flex items-center space-x-2.5">
                <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                <div>
                  <p className="text-zinc-200 font-medium">Vector Indexing Engine</p>
                  <p className="text-[10px] text-zinc-500 font-mono">Qdrant / Memory Store</p>
                </div>
              </div>
              <div className="text-right font-mono">
                <p className="text-emerald-400 font-medium">Active</p>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Recent Collections & Documents Summary */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Collections Overview */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <h2 className="text-base font-semibold text-white flex items-center space-x-2">
            <FolderArchive className="w-4 h-4 text-purple-400" />
            <span>Active Collections ({collections.length})</span>
          </h2>

          {collections.length === 0 ? (
            <div className="p-8 text-center border border-white/5 rounded-xl bg-zinc-950/40 text-xs text-zinc-500">
              No collections found in backend repository.
            </div>
          ) : (
            <div className="space-y-2">
              {collections.slice(0, 5).map((col) => (
                <div key={col.id} className="p-3 rounded-xl bg-zinc-900/60 border border-white/5 flex items-center justify-between text-xs">
                  <div>
                    <p className="font-semibold text-white">{col.name}</p>
                    <p className="text-[11px] text-zinc-400">{col.description || 'No description'}</p>
                  </div>
                  <span className="font-mono text-purple-400 text-xs">{col.documents_count} docs</span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Ingested Documents Overview */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <h2 className="text-base font-semibold text-white flex items-center space-x-2">
            <FileText className="w-4 h-4 text-indigo-400" />
            <span>Ingested Documents ({documents.length})</span>
          </h2>

          {documents.length === 0 ? (
            <div className="p-8 text-center border border-white/5 rounded-xl bg-zinc-950/40 text-xs text-zinc-500">
              No documents ingested yet.
            </div>
          ) : (
            <div className="space-y-2">
              {documents.slice(0, 5).map((doc) => (
                <div key={doc.id} className="p-3 rounded-xl bg-zinc-900/60 border border-white/5 flex items-center justify-between text-xs">
                  <div>
                    <p className="font-semibold text-white">{doc.name}</p>
                    <p className="text-[11px] text-zinc-400">ID: {doc.id}</p>
                  </div>
                  <span className="font-mono text-emerald-400 text-xs">{doc.chunks_count} chunks</span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
