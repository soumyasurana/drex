'use client';

import React, { useState } from 'react';
import { motion } from 'framer-motion';
import {
  Settings,
  Key,
  Sliders,
  Cpu,
  Layers,
  Save,
  CheckCircle2,
  Sparkles,
  Server,
  Database,
  Globe,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';
import { toast } from 'sonner';
import { GATEWAY_URL } from '@/lib/api';

export default function SettingsPage() {
  const { settings, updateSettings } = useAppStore();
  const [form, setForm] = useState(settings);

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    updateSettings(form);
    toast.success('System Settings updated successfully!');
  };

  return (
    <div className="space-y-8 max-w-4xl">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold tracking-tight text-white flex items-center space-x-2">
          <Settings className="w-6 h-6 text-indigo-400" />
          <span>Platform Settings & API Keys</span>
        </h1>
        <p className="text-sm text-zinc-400 mt-1">
          Configure model providers, chunking parameters, vector dimensions, and Gateway REST integration.
        </p>
      </div>

      <form onSubmit={handleSave} className="space-y-6">
        {/* Model Providers */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <h2 className="text-base font-bold text-white flex items-center space-x-2 border-b border-white/10 pb-3">
            <Cpu className="w-5 h-5 text-indigo-400" />
            <span>AI Model & Embedding Provider Config</span>
          </h2>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
            <div>
              <label className="block text-zinc-300 font-semibold mb-1">LLM Provider</label>
              <select
                value={form.llm_provider}
                onChange={(e) => setForm({ ...form, llm_provider: e.target.value as any })}
                className="w-full p-2.5 bg-zinc-900 border border-white/10 rounded-xl text-zinc-100 focus:outline-none focus:border-indigo-500/50"
              >
                <option value="openai">OpenAI (GPT-4o / GPT-4o-mini)</option>
                <option value="anthropic">Anthropic (Claude 3.5 Sonnet)</option>
                <option value="gemini">Google Gemini (Gemini 1.5 Pro)</option>
                <option value="ollama">Ollama Local (Llama 3 / Mistral)</option>
              </select>
            </div>

            <div>
              <label className="block text-zinc-300 font-semibold mb-1">Embedding Provider</label>
              <select
                value={form.embedding_provider}
                onChange={(e) => setForm({ ...form, embedding_provider: e.target.value as any })}
                className="w-full p-2.5 bg-zinc-900 border border-white/10 rounded-xl text-zinc-100 focus:outline-none focus:border-indigo-500/50"
              >
                <option value="openai">OpenAI (text-embedding-3-small)</option>
                <option value="ollama">Ollama Local (nomic-embed-text)</option>
                <option value="huggingface">HuggingFace (bge-large-en-v1.5)</option>
              </select>
            </div>
          </div>
        </div>

        {/* Chunking & Retrieval Parameters */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <h2 className="text-base font-bold text-white flex items-center space-x-2 border-b border-white/10 pb-3">
            <Sliders className="w-5 h-5 text-purple-400" />
            <span>Chunking & Retrieval Execution Defaults</span>
          </h2>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6 text-xs">
            <div>
              <div className="flex justify-between text-zinc-300 font-semibold mb-1">
                <span>Chunk Size (tokens)</span>
                <span className="font-mono text-purple-400">{form.chunk_size} tokens</span>
              </div>
              <input
                type="range"
                min="128"
                max="2048"
                step="64"
                value={form.chunk_size}
                onChange={(e) => setForm({ ...form, chunk_size: parseInt(e.target.value) })}
                className="w-full accent-purple-500"
              />
            </div>

            <div>
              <div className="flex justify-between text-zinc-300 font-semibold mb-1">
                <span>Chunk Overlap (tokens)</span>
                <span className="font-mono text-purple-400">{form.chunk_overlap} tokens</span>
              </div>
              <input
                type="range"
                min="0"
                max="256"
                step="16"
                value={form.chunk_overlap}
                onChange={(e) => setForm({ ...form, chunk_overlap: parseInt(e.target.value) })}
                className="w-full accent-purple-500"
              />
            </div>

            <div>
              <div className="flex justify-between text-zinc-300 font-semibold mb-1">
                <span>Default Retrieval Top K</span>
                <span className="font-mono text-indigo-400">{form.retrieval_k} chunks</span>
              </div>
              <input
                type="range"
                min="1"
                max="20"
                step="1"
                value={form.retrieval_k}
                onChange={(e) => setForm({ ...form, retrieval_k: parseInt(e.target.value) })}
                className="w-full accent-indigo-500"
              />
            </div>

            <div>
              <div className="flex justify-between text-zinc-300 font-semibold mb-1">
                <span>Temperature</span>
                <span className="font-mono text-indigo-400">{form.temperature}</span>
              </div>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={form.temperature}
                onChange={(e) => setForm({ ...form, temperature: parseFloat(e.target.value) })}
                className="w-full accent-indigo-500"
              />
            </div>
          </div>
        </div>

        {/* API Credentials & Gateway URL */}
        <div className="glass-panel rounded-2xl p-6 space-y-4">
          <h2 className="text-base font-bold text-white flex items-center space-x-2 border-b border-white/10 pb-3">
            <Key className="w-5 h-5 text-amber-400" />
            <span>API Keys & Gateway REST Endpoint</span>
          </h2>

          <div className="space-y-4 text-xs">
            <div>
              <label className="block text-zinc-300 font-semibold mb-1">Rust Gateway API Endpoint</label>
              <input
                type="text"
                value={form.gateway_url}
                onChange={(e) => setForm({ ...form, gateway_url: e.target.value })}
                placeholder={GATEWAY_URL}
                className="w-full p-2.5 bg-zinc-900 border border-white/10 rounded-xl text-zinc-100 font-mono text-xs focus:outline-none focus:border-amber-500/50"
              />
            </div>

            <div>
              <label className="block text-zinc-300 font-semibold mb-1">Contextra Platform API Key</label>
              <input
                type="password"
                value={form.api_key}
                onChange={(e) => setForm({ ...form, api_key: e.target.value })}
                className="w-full p-2.5 bg-zinc-900 border border-white/10 rounded-xl text-zinc-100 font-mono text-xs focus:outline-none focus:border-amber-500/50"
              />
            </div>
          </div>
        </div>

        {/* Submit */}
        <div className="flex justify-end">
          <button
            type="submit"
            className="px-6 py-3 rounded-xl bg-gradient-to-r from-indigo-600 via-purple-600 to-pink-600 hover:from-indigo-500 hover:to-pink-500 text-white font-semibold text-sm flex items-center space-x-2 shadow-lg shadow-indigo-500/25 transition-all hover:scale-[1.02]"
          >
            <Save className="w-4 h-4" />
            <span>Save All Configurations</span>
          </button>
        </div>
      </form>
    </div>
  );
}
