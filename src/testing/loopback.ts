import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export enum LoopbackType {
  LOCAL = 'local',           // Local loopback (near-end)
  REMOTE = 'remote',         // Remote loopback (far-end)
  LINE = 'line',             // Line loopback
  PAYLOAD = 'payload',       // Payload loopback
  NETWORK = 'network'        // Network loopback
}

export enum LoopbackCommand {
  ACTIVATE_LOCAL = 'activate_local',
  DEACTIVATE_LOCAL = 'deactivate_local',
  ACTIVATE_REMOTE = 'activate_remote',
  DEACTIVATE_REMOTE = 'deactivate_remote',
  ACTIVATE_LINE = 'activate_line',
  DEACTIVATE_LINE = 'deactivate_line'
}

export interface LoopbackTest {
  id: string;
  channelId: number;
  type: LoopbackType;
  status: 'idle' | 'activating' | 'active' | 'deactivating' | 'failed';
  startTime?: Date;
  endTime?: Date;
  duration?: number;
  packetsTransmitted: number;
  packetsReceived: number;
  packetLoss: number;
  latency?: number;
  errorCount: number;
  lastError?: string;
}

export interface TDMLoopbackCommand {
  command: LoopbackCommand;
  channelId: number;
  timeSlot?: number;
  duration?: number; // Duration in seconds, 0 for indefinite
}

export class LoopbackTester extends EventEmitter {
  private tests: Map<string, LoopbackTest> = new Map();
  private logger: Logger;
  private nextTestId = 1;
  private testIntervals: Map<string, NodeJS.Timeout> = new Map();

  constructor(logger: Logger) {
    super();
    this.logger = logger;
  }

  // Start a loopback test
  startLoopbackTest(channelId: number, type: LoopbackType, duration = 0): string {
    const existingTest = this.findActiveTestForChannel(channelId);
    if (existingTest) {
      throw new Error(`Channel ${channelId} already has an active loopback test`);
    }

    const testId = this.generateTestId();
    const test: LoopbackTest = {
      id: testId,
      channelId,
      type,
      status: 'activating',
      startTime: new Date(),
      packetsTransmitted: 0,
      packetsReceived: 0,
      packetLoss: 0,
      errorCount: 0
    };

    this.tests.set(testId, test);
    this.logger.info(`Starting ${type} loopback test on channel ${channelId}`, { testId });

    // Send TDM loopback command
    this.sendTDMLoopbackCommand(channelId, type, true);

    // Start test monitoring
    this.startTestMonitoring(testId);

    // Set automatic stop timer if duration is specified
    if (duration > 0) {
      setTimeout(() => {
        this.stopLoopbackTest(testId);
      }, duration * 1000);
    }

    this.emit('testStarted', test);
    return testId;
  }

  // Stop a loopback test
  stopLoopbackTest(testId: string): boolean {
    const test = this.tests.get(testId);
    if (!test) {
      return false;
    }

    if (test.status === 'idle') {
      return true;
    }

    test.status = 'deactivating';
    test.endTime = new Date();
    if (test.startTime) {
      test.duration = test.endTime.getTime() - test.startTime.getTime();
    }

    // Send TDM loopback deactivation command
    this.sendTDMLoopbackCommand(test.channelId, test.type, false);

    // Stop test monitoring
    this.stopTestMonitoring(testId);

    // Calculate final statistics
    this.calculateFinalStats(test);

    this.logger.info(`Stopped loopback test ${testId}`, {
      channelId: test.channelId,
      type: test.type,
      duration: test.duration,
      packetLoss: test.packetLoss
    });

    this.emit('testStopped', test);
    return true;
  }

  // Send TDM loopback command through interface
  private sendTDMLoopbackCommand(channelId: number, type: LoopbackType, activate: boolean): void {
    let command: LoopbackCommand;

    switch (type) {
      case LoopbackType.LOCAL:
        command = activate ? LoopbackCommand.ACTIVATE_LOCAL : LoopbackCommand.DEACTIVATE_LOCAL;
        break;
      case LoopbackType.REMOTE:
        command = activate ? LoopbackCommand.ACTIVATE_REMOTE : LoopbackCommand.DEACTIVATE_REMOTE;
        break;
      case LoopbackType.LINE:
        command = activate ? LoopbackCommand.ACTIVATE_LINE : LoopbackCommand.DEACTIVATE_LINE;
        break;
      default:
        throw new Error(`Unsupported loopback type: ${type}`);
    }

    const tdmCommand: TDMLoopbackCommand = {
      command,
      channelId,
      timeSlot: channelId
    };

    this.emit('tdmLoopbackCommand', tdmCommand);
    this.logger.debug(`Sent TDM loopback command`, tdmCommand);
  }

  // Start monitoring a test
  private startTestMonitoring(testId: string): void {
    const interval = setInterval(() => {
      this.updateTestStats(testId);
    }, 1000); // Update every second

    this.testIntervals.set(testId, interval);
  }

  // Stop monitoring a test
  private stopTestMonitoring(testId: string): void {
    const interval = this.testIntervals.get(testId);
    if (interval) {
      clearInterval(interval);
      this.testIntervals.delete(testId);
    }
  }

  // Update test statistics
  private updateTestStats(testId: string): void {
    const test = this.tests.get(testId);
    if (!test || test.status !== 'active') {
      return;
    }

    // Simulate packet transmission and reception
    test.packetsTransmitted += 10;
    
    // Simulate some packet loss and errors
    const packetLossRate = Math.random() * 0.01; // 0-1% packet loss
    const packetsLost = Math.floor(test.packetsTransmitted * packetLossRate);
    test.packetsReceived = test.packetsTransmitted - packetsLost;
    test.packetLoss = (packetsLost / test.packetsTransmitted) * 100;

    // Simulate latency
    test.latency = 1 + Math.random() * 2; // 1-3ms latency

    // Check for errors
    if (test.packetLoss > 5) { // If packet loss > 5%
      test.errorCount++;
      test.lastError = `High packet loss: ${test.packetLoss.toFixed(2)}%`;
      this.emit('testError', test);
    }

    this.emit('testUpdated', test);
  }

  // Calculate final statistics
  private calculateFinalStats(test: LoopbackTest): void {
    if (test.packetsTransmitted > 0) {
      const packetsLost = test.packetsTransmitted - test.packetsReceived;
      test.packetLoss = (packetsLost / test.packetsTransmitted) * 100;
    }

    test.status = 'idle';
  }

  // Handle received TDM loopback responses
  handleTDMLoopbackResponse(channelId: number, success: boolean, data?: Buffer): void {
    const test = this.findActiveTestForChannel(channelId);
    if (!test) {
      return;
    }

    if (test.status === 'activating') {
      if (success) {
        test.status = 'active';
        this.logger.info(`Loopback test ${test.id} activated successfully`);
      } else {
        test.status = 'failed';
        test.lastError = 'Failed to activate loopback';
        this.logger.error(`Loopback test ${test.id} activation failed`);
        this.emit('testFailed', test);
      }
    }

    // Process loopback data if provided
    if (data && test.status === 'active') {
      this.processLoopbackData(test, data);
    }
  }

  // Process received loopback data
  private processLoopbackData(test: LoopbackTest, data: Buffer): void {
    // Analyze the returned data for errors
    // This is a simplified implementation
    test.packetsReceived++;
    
    // Check for bit errors in the data
    // In a real implementation, this would compare against known test patterns
    const hasErrors = this.detectBitErrors(data);
    if (hasErrors) {
      test.errorCount++;
      test.lastError = 'Bit errors detected in loopback data';
      this.emit('testError', test);
    }
  }

  // Detect bit errors in received data (simplified)
  private detectBitErrors(data: Buffer): boolean {
    // Simplified error detection - in reality this would use known test patterns
    return Math.random() < 0.001; // 0.1% chance of detecting errors
  }

  // Find active test for a channel
  private findActiveTestForChannel(channelId: number): LoopbackTest | undefined {
    for (const test of this.tests.values()) {
      if (test.channelId === channelId && 
          (test.status === 'activating' || test.status === 'active' || test.status === 'deactivating')) {
        return test;
      }
    }
    return undefined;
  }

  // Get test by ID
  getTest(testId: string): LoopbackTest | undefined {
    return this.tests.get(testId);
  }

  // Get all tests
  getAllTests(): LoopbackTest[] {
    return Array.from(this.tests.values());
  }

  // Get active tests
  getActiveTests(): LoopbackTest[] {
    return Array.from(this.tests.values()).filter(test => 
      test.status === 'activating' || test.status === 'active' || test.status === 'deactivating'
    );
  }

  // Get tests for a specific channel
  getTestsForChannel(channelId: number): LoopbackTest[] {
    return Array.from(this.tests.values()).filter(test => test.channelId === channelId);
  }

  // Check if any tests are running
  hasActiveTests(): boolean {
    return this.getActiveTests().length > 0;
  }

  // Stop all active tests
  stopAllTests(): number {
    const activeTests = this.getActiveTests();
    let stoppedCount = 0;

    for (const test of activeTests) {
      if (this.stopLoopbackTest(test.id)) {
        stoppedCount++;
      }
    }

    return stoppedCount;
  }

  // Generate test ID
  private generateTestId(): string {
    return `LOOP-${Date.now()}-${this.nextTestId++}`;
  }

  // Clean up old completed tests
  cleanupOldTests(maxAge: number = 24 * 60 * 60 * 1000): number { // Default 24 hours
    const cutoffTime = new Date(Date.now() - maxAge);
    let cleanedCount = 0;

    for (const [id, test] of this.tests) {
      if (test.status === 'idle' && test.endTime && test.endTime < cutoffTime) {
        this.tests.delete(id);
        cleanedCount++;
      }
    }

    if (cleanedCount > 0) {
      this.logger.info(`Cleaned up ${cleanedCount} old loopback tests`);
    }

    return cleanedCount;
  }

  // Export test results
  exportTestResults(includeActive = false): any[] {
    const tests = includeActive ? this.getAllTests() : 
      this.getAllTests().filter(test => test.status === 'idle');
    
    return tests.map(test => ({
      id: test.id,
      channelId: test.channelId,
      type: test.type,
      status: test.status,
      startTime: test.startTime?.toISOString(),
      endTime: test.endTime?.toISOString(),
      duration: test.duration,
      packetsTransmitted: test.packetsTransmitted,
      packetsReceived: test.packetsReceived,
      packetLoss: test.packetLoss,
      latency: test.latency,
      errorCount: test.errorCount,
      lastError: test.lastError
    }));
  }
}