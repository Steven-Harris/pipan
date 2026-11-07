import { invoke } from '@tauri-apps/api/core';
import type {
  AnalyticsPeriod,
  RecordUsageInput,
  UsageAnalytics,
  UsageBucket
} from '#lib/types/usage.ts';

class UsageStore {
  analytics = $state<UsageAnalytics | null>(null);
  loading = $state<boolean>(false);
  error = $state<string | null>(null);
  selectedPeriod = $state<AnalyticsPeriod>('day');
  subPeriod = $state<'primary' | 'secondary'>('primary');

  private initialized = false;

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;
    await this.refresh();
  }

  async refresh(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const data = await invoke<UsageAnalytics>('get_usage_analytics');
      this.analytics = data;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async rescan(): Promise<number> {
    this.loading = true;
    this.error = null;
    try {
      const count = await invoke<number>('rescan_usage_files');
      await this.refresh();
      return count;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return 0;
    } finally {
      this.loading = false;
    }
  }

  async recordUsage(entry: RecordUsageInput): Promise<void> {
    try {
      await invoke('record_usage_entry', { entry });
      // Non-blocking refresh
      void this.refresh();
    } catch {
      // Non-fatal if recording in browser test mode
    }
  }

  setPeriod(period: AnalyticsPeriod): void {
    this.selectedPeriod = period;
    this.subPeriod = 'primary';
  }

  setSubPeriod(sub: 'primary' | 'secondary'): void {
    this.subPeriod = sub;
  }

  readonly activeBuckets = $derived.by((): UsageBucket[] => {
    if (!this.analytics) return [];
    const p = this.selectedPeriod;
    const isPrimary = this.subPeriod === 'primary';

    if (p === 'day') {
      return isPrimary ? this.analytics.day.hourlyToday : this.analytics.day.daily30Days;
    }
    if (p === 'week') {
      return isPrimary ? this.analytics.week.dailyThisWeek : this.analytics.week.weekly12Weeks;
    }
    if (p === 'month') {
      return isPrimary ? this.analytics.month.dailyThisMonth : this.analytics.month.monthly12Months;
    }
    return isPrimary ? this.analytics.year.monthlyThisYear : this.analytics.year.yearlyAll;
  });

  readonly activePeriodTotals = $derived.by(() => {
    if (!this.analytics) {
      return {
        costUsd: 0,
        totalTokens: 0,
        inputTokens: 0,
        outputTokens: 0,
        cacheReadTokens: 0,
        cacheWriteTokens: 0
      };
    }
    const p = this.selectedPeriod;
    const current =
      p === 'day'
        ? this.analytics.day
        : p === 'week'
          ? this.analytics.week
          : p === 'month'
            ? this.analytics.month
            : this.analytics.year;

    return {
      costUsd: current.totalCostUsd,
      totalTokens: current.totalTokens,
      inputTokens: current.inputTokens,
      outputTokens: current.outputTokens,
      cacheReadTokens: current.cacheReadTokens,
      cacheWriteTokens: current.cacheWriteTokens
    };
  });
}

export const usageStore = new UsageStore();
