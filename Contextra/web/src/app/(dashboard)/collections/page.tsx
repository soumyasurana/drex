'use client';

import React, { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  FolderArchive,
  Plus,
  FileText,
  Layers,
  Edit2,
  Trash2,
  Search,
  X,
  RefreshCw,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';
import { CollectionResource } from '@/types';
import { api } from '@/lib/api';
import { toast } from 'sonner';

export default function CollectionsPage() {
  const { collections, collectionsLoading, settings, fetchCollections, addCollection, deleteCollection, updateCollection } = useAppStore();
  const [searchQuery, setSearchQuery] = useState('');
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [editingCol, setEditingCol] = useState<CollectionResource | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Form states
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');

  useEffect(() => {
    fetchCollections();
  }, [fetchCollections]);

  const filteredCollections = collections.filter(
    (c) =>
      c.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      c.description.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || isSubmitting) return;

    setIsSubmitting(true);
    try {
      if (editingCol) {
        updateCollection(editingCol.id, name, description);
        toast.success(`Updated collection "${name}"`);
        setEditingCol(null);
      } else {
        const created = await api.createCollection(name, { description }, settings.api_key);
        if (created) {
          addCollection(created);
          toast.success(`Created collection "${name}"`);
        } else {
          // Fallback local creation if backend offline
          const newCol: CollectionResource = {
            id: `col_${Date.now()}`,
            name,
            description,
            documents_count: 0,
            chunks_count: 0,
            created_at: new Date().toISOString().split('T')[0],
          };
          addCollection(newCol);
          toast.success(`Created collection "${name}" (local)`);
        }
      }

      setName('');
      setDescription('');
      setCreateModalOpen(false);
    } catch {
      toast.error('Failed to create collection');
    } finally {
      setIsSubmitting(false);
    }
  };

  const openEdit = (col: CollectionResource) => {
    setEditingCol(col);
    setName(col.name);
    setDescription(col.description);
    setCreateModalOpen(true);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight text-white flex items-center space-x-2">
            <FolderArchive className="w-6 h-6 text-purple-400" />
            <span>Document Collections</span>
          </h1>
          <p className="text-sm text-zinc-400 mt-1">
            Group vector indexes by domain, access policies, and embedding models.
          </p>
        </div>

        <div className="flex items-center space-x-3">
          <button
            onClick={() => fetchCollections()}
            className="px-3.5 py-2.5 rounded-xl glass-card text-xs text-zinc-300 hover:text-white flex items-center space-x-2 transition-colors"
          >
            <RefreshCw className={`w-4 h-4 ${collectionsLoading ? 'animate-spin' : ''}`} />
            <span>Sync</span>
          </button>
          <button
            onClick={() => {
              setEditingCol(null);
              setName('');
              setDescription('');
              setCreateModalOpen(true);
            }}
            className="px-4 py-2.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white font-medium text-sm flex items-center justify-center space-x-2 shadow-lg shadow-purple-500/25 transition-all hover:scale-[1.02]"
          >
            <Plus className="w-4 h-4" />
            <span>New Collection</span>
          </button>
        </div>
      </div>

      {/* Search Bar */}
      <div className="glass-panel p-4 rounded-2xl">
        <div className="relative w-full max-w-md">
          <Search className="w-4 h-4 text-zinc-400 absolute left-3 top-3" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Filter collections..."
            className="w-full pl-9 pr-4 py-2 bg-zinc-900/80 border border-white/10 rounded-xl text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:border-purple-500/50"
          />
        </div>
      </div>

      {/* Collection Grid */}
      {collectionsLoading ? (
        <div className="p-12 text-center text-zinc-400 text-sm flex items-center justify-center space-x-2">
          <RefreshCw className="w-4 h-4 animate-spin text-purple-400" />
          <span>Loading collections from backend...</span>
        </div>
      ) : filteredCollections.length === 0 ? (
        <div className="glass-panel rounded-2xl p-12 text-center text-zinc-500 space-y-3">
          <FolderArchive className="w-12 h-12 text-zinc-600 mx-auto" />
          <p className="text-sm font-semibold text-zinc-300">No collections available</p>
          <p className="text-xs text-zinc-500">Create a collection to organize your vector embeddings.</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {filteredCollections.map((col, idx) => (
            <motion.div
              key={col.id}
              initial={{ opacity: 0, y: 15 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.2, delay: idx * 0.05 }}
              className="glass-card glass-card-hover rounded-2xl p-6 flex flex-col justify-between space-y-4 group relative"
            >
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <div className="w-10 h-10 rounded-xl bg-purple-500/10 border border-purple-500/20 text-purple-400 flex items-center justify-center shadow-inner">
                    <FolderArchive className="w-5 h-5" />
                  </div>

                  <div className="flex items-center space-x-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={() => openEdit(col)}
                      className="p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800"
                      title="Rename / Edit"
                    >
                      <Edit2 className="w-3.5 h-3.5" />
                    </button>
                    <button
                      onClick={() => {
                        deleteCollection(col.id);
                        toast.success(`Deleted collection "${col.name}"`);
                      }}
                      className="p-1.5 rounded-lg text-zinc-400 hover:text-rose-400 hover:bg-rose-900/40"
                      title="Delete"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                <div>
                  <h3 className="text-base font-bold text-white group-hover:text-purple-300 transition-colors">
                    {col.name}
                  </h3>
                  <p className="text-xs text-zinc-400 mt-1 line-clamp-2">{col.description || 'No description provided'}</p>
                </div>
              </div>

              <div className="pt-4 border-t border-white/5 grid grid-cols-3 gap-2 text-xs text-zinc-400 font-mono">
                <div className="flex flex-col">
                  <span className="text-[10px] uppercase text-zinc-500 font-sans">Docs</span>
                  <span className="text-zinc-200 font-semibold flex items-center space-x-1 mt-0.5">
                    <FileText className="w-3 h-3 text-purple-400" />
                    <span>{col.documents_count}</span>
                  </span>
                </div>
                <div className="flex flex-col">
                  <span className="text-[10px] uppercase text-zinc-500 font-sans">Chunks</span>
                  <span className="text-zinc-200 font-semibold flex items-center space-x-1 mt-0.5">
                    <Layers className="w-3 h-3 text-indigo-400" />
                    <span>{col.chunks_count}</span>
                  </span>
                </div>
                <div className="flex flex-col">
                  <span className="text-[10px] uppercase text-zinc-500 font-sans">ID</span>
                  <span className="text-zinc-400 text-[10px] truncate mt-0.5">{col.id}</span>
                </div>
              </div>
            </motion.div>
          ))}
        </div>
      )}

      {/* Create / Edit Modal */}
      <AnimatePresence>
        {createModalOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-md">
            <motion.div
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              className="w-full max-w-md glass-panel rounded-2xl p-6 border border-white/10 shadow-2xl relative"
            >
              <div className="flex items-center justify-between border-b border-white/10 pb-4 mb-4">
                <h3 className="text-lg font-bold text-white flex items-center space-x-2">
                  <FolderArchive className="w-5 h-5 text-purple-400" />
                  <span>{editingCol ? 'Edit Collection' : 'Create New Collection'}</span>
                </h3>
                <button
                  onClick={() => setCreateModalOpen(false)}
                  className="p-1 rounded-lg text-zinc-400 hover:text-white"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>

              <form onSubmit={handleCreate} className="space-y-4">
                <div>
                  <label className="block text-xs font-semibold text-zinc-300 mb-1">Collection Name</label>
                  <input
                    type="text"
                    required
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="e.g. Legal Contracts v2"
                    className="w-full px-3.5 py-2 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 focus:outline-none focus:border-purple-500/50"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-zinc-300 mb-1">Description</label>
                  <textarea
                    rows={3}
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    placeholder="Brief description of the documents stored in this collection..."
                    className="w-full px-3.5 py-2 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 focus:outline-none focus:border-purple-500/50"
                  />
                </div>

                <div className="flex items-center justify-end space-x-3 pt-4 border-t border-white/10">
                  <button
                    type="button"
                    onClick={() => setCreateModalOpen(false)}
                    className="px-4 py-2 bg-zinc-800 text-zinc-300 hover:text-white rounded-xl text-xs font-medium"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={isSubmitting}
                    className="px-4 py-2 bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white rounded-xl text-xs font-semibold shadow-md flex items-center space-x-2"
                  >
                    {isSubmitting && <RefreshCw className="w-3.5 h-3.5 animate-spin" />}
                    <span>{editingCol ? 'Save Changes' : 'Create Collection'}</span>
                  </button>
                </div>
              </form>
            </motion.div>
          </div>
        )}
      </AnimatePresence>
    </div>
  );
}
