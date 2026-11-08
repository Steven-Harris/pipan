/**
 * Types for System Dependency Doctor & Auto-Fixer (`src-tauri/src/doctor.rs`).
 */

export type DoctorCheckStatus = 'pass' | 'warn' | 'fail';

export type DoctorCheckId =
  | 'git'
  | 'node_npm'
  | 'pi_cli'
  | 'pi_auth'
  | 'gh_cli'
  | 'gh_auth'
  | 'gh_webhook';

export interface DoctorCheckResult {
  id: DoctorCheckId | string;
  title: string;
  status: DoctorCheckStatus;
  summary: string;
  details?: string;
  manualInstructions: string[];
  fixAction: 'auto_command' | 'open_pi_auth' | 'open_gh_auth' | 'git_config' | string;
  fixLabel: string;
  canAutoFix: boolean;
}

export interface DoctorReport {
  overallStatus: DoctorCheckStatus;
  passCount: number;
  warnCount: number;
  failCount: number;
  checkedAt: number;
  checks: DoctorCheckResult[];
}

export interface DoctorFixLogEvent {
  checkId: string;
  stream: 'stdout' | 'stderr' | string;
  line: string;
  timestamp: number;
}

export interface DoctorFixResult {
  checkId: string;
  success: boolean;
  message: string;
  outputLines: string[];
  openFlow?: 'pi_auth' | 'gh_auth' | string | null;
  updatedReport?: DoctorReport | null;
}
