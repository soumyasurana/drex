'use client';

import React from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import {
  LayoutDashboard,
  FileText,
  FolderArchive,
  MessageSquare,
  History,
  GitMerge,
  Code2,
  BarChart3,
  Settings,
  Zap,
  Activity,
  Cpu,
  Layers,
  Sparkles,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';

const navItems = [
  { name: 'Dashboard', icon: LayoutDashboard, href: '/dashboard' },
  { name: 'Documents', icon: FileText, href: '/documents' },
  { name: 'Collections', icon: FolderArchive, href: '/collections' },
  { name: 'Playground Chat', icon: MessageSquare, href: '/chat' },
  { name: 'Conversations', icon: History, href: '/conversations' },
  { name: 'Retrieval Explorer', icon: GitMerge, href: '/retrieval' },
  { name: 'Prompt Studio', icon: Code2, href: '/prompts' },
  { name: 'Evaluation', icon: BarChart3, href: '/evaluation' },
  { name: 'Settings', icon: Settings, href: '/settings' },
];

export function Sidebar() {
  const pathname = usePathname();
  const { apiConnected } = useAppStore();

  return (
    <aside className="w-64 h-screen shrink-0 border-r border-white/10 glass-panel flex flex-col justify-between p-4 fixed left-0 top-0 z-40 bg-[#0c0e17]/90">
      <div className="space-y-6">
        {/* Brand Header */}
        <Link href="/" className="flex items-center space-x-3 px-2 pt-1 group">
          <div className="w-9 h-9 rounded-xl bg-gradient-to-tr from-indigo-600 via-purple-600 to-pink-500 flex items-center justify-center shadow-lg shadow-indigo-500/20 group-hover:scale-105 transition-transform">
            <Cpu className="w-5 h-5 text-white" />
          </div>
          <div>
            <div className="flex items-center space-x-2">
              <span className="font-bold text-lg tracking-tight text-white font-sans">Contextra</span>
              <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-indigo-500/20 text-indigo-400 border border-indigo-500/30 rounded-full">
                v1.0
              </span>
            </div>
            <p className="text-[11px] text-zinc-400 font-medium">Context Engineering</p>
          </div>
        </Link>

        {/* Navigation Links */}
        <nav className="space-y-1">
          <div className="px-3 py-1 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
            Platform
          </div>
          {navItems.map((item) => {
            const isActive = pathname === item.href || (item.href !== '/dashboard' && pathname?.startsWith(item.href));
            const Icon = item.icon;
            return (
              <Link
                key={item.href}
                href={item.href}
                className={`flex items-center justify-between px-3 py-2.5 rounded-xl text-sm font-medium transition-all group ${
                  isActive
                    ? 'bg-indigo-600/20 text-white border border-indigo-500/40 shadow-sm shadow-indigo-500/10'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5 border border-transparent'
                }`}
              >
                <div className="flex items-center space-x-3">
                  <Icon
                    className={`w-4 h-4 transition-colors ${
                      isActive ? 'text-indigo-400' : 'text-zinc-500 group-hover:text-zinc-300'
                    }`}
                  />
                  <span>{item.name}</span>
                </div>
                {isActive && (
                  <div className="w-1.5 h-1.5 rounded-full bg-indigo-400 shadow-sm shadow-indigo-400" />
                )}
              </Link>
            );
          })}
        </nav>
      </div>

      {/* Bottom Health & Engine Card */}
      <div className="space-y-3">
        <div className="p-3 rounded-xl bg-zinc-900/80 border border-white/10 space-y-2">
          <div className="flex items-center justify-between text-xs font-medium">
            <span className="text-zinc-300 flex items-center space-x-1.5">
              <Zap className="w-3.5 h-3.5 text-amber-400" />
              <span>Rust Core Backend</span>
            </span>
            <span className="flex items-center space-x-1 text-emerald-400 text-[11px]">
              <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
              <span>Online</span>
            </span>
          </div>

          <div className="grid grid-cols-2 gap-1 text-[11px] text-zinc-400 border-t border-white/5 pt-2">
            <div>Vector: <span className="text-zinc-200 font-mono">Qdrant</span></div>
            <div>Queue: <span className="text-zinc-200 font-mono">Redis</span></div>
          </div>
        </div>

        <div className="flex items-center justify-between px-2 text-[11px] text-zinc-400">
          <span className="truncate">Contextra Platform</span>
          <span className="font-mono text-indigo-400">0.14s avg</span>
        </div>
      </div>
    </aside>
  );
}
