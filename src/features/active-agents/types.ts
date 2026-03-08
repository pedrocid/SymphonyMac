export type LogFilter = "all" | "stdout" | "stderr";

export interface LiveLogEntry {
  line: string;
  ts: string;
}
