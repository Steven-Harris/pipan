export type AnalyticsPeriod = 'day' | 'week' | 'month' | 'year';

export interface UsageBucket {
  key: string;
  label: string;
  startTimestamp: number;
  endTimestamp: number;
  costUsd: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
  messageCount: number;
}

export interface DayAnalytics {
  hourlyToday: UsageBucket[];
  daily30Days: UsageBucket[];
  totalCostUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface WeekAnalytics {
  dailyThisWeek: UsageBucket[];
  weekly12Weeks: UsageBucket[];
  totalCostUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface MonthAnalytics {
  dailyThisMonth: UsageBucket[];
  monthly12Months: UsageBucket[];
  totalCostUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface YearAnalytics {
  monthlyThisYear: UsageBucket[];
  yearlyAll: UsageBucket[];
  totalCostUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface ModelUsageBreakdown {
  provider: string;
  model: string;
  costUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  messageCount: number;
  percentCost: number;
}

export interface SessionUsageBreakdown {
  sessionId: string;
  sessionName: string;
  cwd?: string | null;
  costUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  messageCount: number;
  lastActiveTs: number;
}

export interface UsageTotals {
  totalSpendUsd: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalSessions: number;
  activeSessions: number;
  avgCostPerSession: number;
  totalMessages: number;
}

export interface UsageAnalytics {
  totals: UsageTotals;
  day: DayAnalytics;
  week: WeekAnalytics;
  month: MonthAnalytics;
  year: YearAnalytics;
  byModel: ModelUsageBreakdown[];
  bySession: SessionUsageBreakdown[];
  scannedFilesCount: number;
  newlyIndexedCount: number;
}

export interface RecordUsageInput {
  sessionId: string;
  messageId?: string;
  timestamp?: number;
  provider?: string;
  model?: string;
  input?: number;
  output?: number;
  cacheRead?: number;
  cacheWrite?: number;
  costUsd?: number;
  cwd?: string | null;
}
