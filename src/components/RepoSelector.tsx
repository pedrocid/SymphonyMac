import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Repo {
  full_name: string;
  name: string;
  owner: string;
  description: string | null;
  url: string;
  default_branch: string;
  is_private: boolean;
}

export function RepoSelector({ onSelect }: { onSelect: (repo: string) => void }) {
  const [repos, setRepos] = useState<Repo[]>([]);
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
      const result = await invoke<Repo[]>("list_repos", { filter: null });
      setRepos(result);
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
        <h2 className="text-2xl font-bold text-[#e6edf3] mb-2">Select Repository</h2>
        <p className="text-[#8b949e] mb-6">
          Choose a repository to orchestrate agent work on its issues.
        </p>

        <input
          type="text"
          placeholder="Filter repositories..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="w-full px-4 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] placeholder-[#484f58] mb-4 outline-none focus:border-[#58a6ff] transition-colors"
        />

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
          {filtered.map((repo) => (
            <button
              key={repo.full_name}
              onClick={() => onSelect(repo.full_name)}
              className="w-full text-left p-4 bg-[#161b22] border border-[#30363d] rounded-lg hover:border-[#58a6ff] transition-colors group"
            >
              <div className="flex items-center justify-between">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-[#58a6ff] font-medium">{repo.full_name}</span>
                    {repo.is_private && (
                      <span className="text-xs px-2 py-0.5 rounded-full border border-[#30363d] text-[#8b949e]">
                        Private
                      </span>
                    )}
                  </div>
                  {repo.description && (
                    <p className="text-sm text-[#8b949e] mt-1 truncate">
                      {repo.description}
                    </p>
                  )}
                </div>
                <span className="text-[#8b949e] group-hover:text-[#58a6ff] transition-colors ml-4">
                  &rarr;
                </span>
              </div>
            </button>
          ))}
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
