export interface RunSummary {
  id: string;
  repo: string;
  issue_number: number;
  issue_title: string;
  status: string;
  stage: string;
  started_at: string;
  finished_at: string | null;
  workspace_path: string;
  error: string | null;
  attempt: number;
  max_retries: number;
  command_display: string | null;
  agent_type: string;
  last_log_line: string | null;
  log_count: number;
  activity: string | null;
  last_log_timestamp: string | null;
  skipped_stages: string[];
  pending_next_stage: string | null;
}

export interface OrchestratorOverview {
  is_running: boolean;
  repos: string[];
  runs: RunSummary[];
  config: {
    max_concurrent: number;
    [key: string]: unknown;
  };
  total_completed: number;
  total_failed: number;
  active_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  total_runtime_secs: number;
}
