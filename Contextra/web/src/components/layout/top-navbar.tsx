'use client';

import React, { useState, useEffect } from 'react';
import {
  Search,
  Bell,
  CheckCircle2,
  Sparkles,
  Server,
  AlertTriangle,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';

function GithubIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" />
      <path d="M9 18c-4.51 2-5-2-7-2" />
    </svg>
  );
}

export function TopNavbar() {
  const { setCommandPaletteOpen, settings, apiConnected, checkApiHealth } = useAppStore();
  const [notificationsOpen, setNotificationsOpen] = useState(false);

  useEffect(() => {
    checkApiHealth();
    const interval = setInterval(checkApiHealth, 15000);
    return () => clearInterval(interval);
  }, [checkApiHealth]);

  const notifications = [
    { id: 1, title: 'Eval Suite Passed', time: '10m ago', unread: true },
    { id: 2, title: '84 Chunks Ingested', time: '45m ago', unread: false },
    { id: 3, title: 'Qdrant HNSW Reindexed', time: '2h ago', unread: false },
  ];

  return (
    <header className="h-16 border-b border-white/10 glass-panel sticky top-0 z-30 px-6 flex items-center justify-between bg-[#090a0f]/80 backdrop-blur-xl">
      {/* Search / Command Bar Trigger */}
      <button
        onClick={() => setCommandPaletteOpen(true)}
        className="flex items-center space-x-3 px-3.5 py-1.5 rounded-xl bg-zinc-900/90 border border-white/10 hover:border-indigo-500/40 text-zinc-400 hover:text-zinc-200 text-sm transition-all w-72 group shadow-inner"
      >
        <Search className="w-4 h-4 text-zinc-500 group-hover:text-indigo-400" />
        <span className="truncate">Search commands, docs...</span>
        <kbd className="ml-auto px-1.5 py-0.5 text-[10px] font-mono font-semibold bg-zinc-800 text-zinc-400 rounded border border-zinc-700">
          ⌘K
        </kbd>
      </button>

      {/* Right Navbar Controls */}
      <div className="flex items-center space-x-4">
        {/* Gateway Connection Status Badge */}
        <div
          className={`flex items-center space-x-1.5 px-3 py-1 rounded-full text-xs font-semibold border ${
            apiConnected
              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
              : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
          }`}
          title={apiConnected ? 'Rust Gateway Connected' : 'Rust Gateway Disconnected'}
        >
          <Server className="w-3.5 h-3.5" />
          <span>{apiConnected ? 'API Connected' : 'API Offline'}</span>
        </div>

        {/* Active Provider Badge */}
        <div className="hidden md:flex items-center space-x-2 px-3 py-1 rounded-full bg-indigo-500/10 border border-indigo-500/20 text-xs font-medium text-indigo-300">
          <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
          <span className="capitalize">{settings.llm_provider}</span>
          <span className="text-zinc-400">•</span>
          <span className="text-zinc-400 font-mono text-[11px]">{settings.llm_model}</span>
        </div>

        {/* GitHub Link */}
        <a
          href="https://github.com/soumyasurana/Contextra"
          target="_blank"
          rel="noopener noreferrer"
          className="p-2 rounded-xl text-zinc-400 hover:text-white hover:bg-white/5 border border-transparent hover:border-white/10 transition-all"
          title="View GitHub Repository"
        >
          <GithubIcon className="w-4 h-4" />
        </a>

        {/* Notifications Dropdown */}
        <div className="relative">
          <button
            onClick={() => setNotificationsOpen(!notificationsOpen)}
            className="p-2 rounded-xl text-zinc-400 hover:text-white hover:bg-white/5 border border-transparent hover:border-white/10 transition-all relative"
          >
            <Bell className="w-4 h-4" />
            <span className="absolute top-1.5 right-1.5 w-2 h-2 rounded-full bg-indigo-500" />
          </button>

          {notificationsOpen && (
            <div className="absolute right-0 mt-2 w-80 rounded-2xl glass-panel border border-white/10 p-3 shadow-2xl space-y-2 z-50 bg-zinc-950">
              <div className="flex items-center justify-between px-2 pb-2 border-b border-white/10 text-xs font-semibold text-zinc-300">
                <span>System Notifications</span>
                <span className="text-indigo-400 text-[11px]">3 New</span>
              </div>
              <div className="space-y-1">
                {notifications.map((n) => (
                  <div
                    key={n.id}
                    className="p-2 rounded-xl hover:bg-white/5 flex items-start space-x-2 text-xs transition-colors"
                  >
                    <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                    <div className="flex-1">
                      <p className="text-zinc-200 font-medium">{n.title}</p>
                      <p className="text-zinc-400 text-[10px]">{n.time}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* User Profile Avatar */}
        <div className="flex items-center space-x-3 pl-2 border-l border-white/10">
          <div className="w-8 h-8 rounded-full bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center text-white font-semibold text-xs ring-2 ring-indigo-500/30">
            SS
          </div>
          <div className="hidden lg:block text-left">
            <p className="text-xs font-semibold text-zinc-200">Soumya Surana</p>
            <p className="text-[10px] text-indigo-400 font-mono">Staff Architect</p>
          </div>
        </div>
      </div>
    </header>
  );
}
