import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Repo } from "../contracts";
import { getConfig } from "../lib/api";

interface RepoEntry {
  full_name: string;
  description?: string | null;
  is_private?: boolean;
  is_local: boolean;
  local_path?: string;
}

export function RepoSelector({
  selectedRepos,
  onToggleRepo,
  onConfirm,
}: {
  selectedRepos: string[];
  onToggleRepo: (repo: string) => void;
  onConfirm: () => void;
}) {
  const [repos, setRepos] = useState<RepoEntry[]>([]);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadRepos();
  }, []);

  async function loadRepos() {
    setLoading(true);
    setError(null);
    try {
      const [ghRepos, config] = await Promise.all([
        invoke<Repo[]>("list_repos", { filter: null }),
        getConfig(),
      ]);

      const localRepos = config.local_repos || {};
      const localFullNames = new Set(Object.keys(localRepos));

      // Build merged list: local repos first, then GitHub repos (deduped)
      const entries: RepoEntry[] = [];

      for (const [fullName, path] of Object.entries(localRepos)) {
        const ghMatch = ghRepos.find((r) => r.full_name === fullName);
        entries.push({
          full_name: fullName,
          description: ghMatch?.description ?? path,
          is_private: ghMatch?.is_private,
          is_local: true,
          local_path: path,
        });
      }

      for (const repo of ghRepos) {
        if (!localFullNames.has(repo.full_name)) {
          entries.push({
            full_name: repo.full_name,
            description: repo.description,
            is_private: repo.is_private,
            is_local: false,
          });
        }
      }

      setRepos(entries);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  const filtered = repos.filter((r) =>
    r.full_name.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <div className="flex-1 overflow-auto p-6">
      <div className="max-w-3xl mx-auto">
        <h2 className="text-2xl font-bold text-[#e6edf3] mb-2">Select Repositories</h2>
        <p className="text-[#8b949e] mb-6">
          Choose one or more repositories to orchestrate agent work on their issues.
        </p>

        <div className="flex items-center gap-3 mb-4">
          <input
            type="text"
            placeholder="Filter repositories..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="flex-1 px-4 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] placeholder-[#484f58] outline-none focus:border-[#58a6ff] transition-colors"
          />
          {selectedRepos.length > 0 && (
            <button
              onClick={onConfirm}
              className="px-4 py-2 bg-[#238636] text-white rounded-lg text-sm font-medium hover:bg-[#2ea043] transition-colors shrink-0"
            >
              Continue with {selectedRepos.length} repo{selectedRepos.length > 1 ? "s" : ""}
            </button>
          )}
        </div>

        {loading && (
          <div className="text-center py-12 text-[#8b949e]">
            Loading repositories...
          </div>
        )}

        {error && (
          <div className="bg-[#f8514926] border border-[#f85149] rounded-lg p-4 mb-4">
            <p className="text-[#f85149] text-sm">{error}</p>
            <button
              onClick={loadRepos}
              className="mt-2 text-sm text-[#58a6ff] hover:underline"
            >
              Retry
            </button>
          </div>
        )}

        <div className="space-y-2">
          {filtered.map((repo) => {
            const isSelected = selectedRepos.includes(repo.full_name);
            return (
              <button
                key={repo.full_name}
                onClick={() => onToggleRepo(repo.full_name)}
                className={`w-full text-left p-4 border rounded-lg transition-colors group ${
                  isSelected
                    ? "bg-[#58a6ff15] border-[#58a6ff]"
                    : "bg-[#161b22] border-[#30363d] hover:border-[#58a6ff]"
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3 flex-1 min-w-0">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => {}}
                      className="accent-[#58a6ff] shrink-0"
                    />
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-[#58a6ff] font-medium">{repo.full_name}</span>
                        {repo.is_local && (
                          <span className="text-xs px-2 py-0.5 rounded-full bg-[#3fb95015] border border-[#3fb950] text-[#3fb950]">
                            Local (worktree)
                          </span>
                        )}
                        {repo.is_private && !repo.is_local && (
                          <span className="text-xs px-2 py-0.5 rounded-full border border-[#30363d] text-[#8b949e]">
                            Private
                          </span>
                        )}
                      </div>
                      {repo.description && (
                        <p className="text-sm text-[#8b949e] mt-1 truncate">
                          {repo.is_local ? repo.local_path : repo.description}
                        </p>
                      )}
                    </div>
                  </div>
                </div>
              </button>
            );
          })}
        </div>

        {!loading && filtered.length === 0 && !error && (
          <div className="text-center py-12 text-[#8b949e]">
            No repositories found.
          </div>
        )}
      </div>
    </div>
  );
}
