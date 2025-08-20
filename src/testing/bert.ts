import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export enum BERTPattern {
  PRBS_9 = 'prbs_9',         // 2^9-1 Pseudo Random Binary Sequence
  PRBS_15 = 'prbs_15',       // 2^15-1 Pseudo Random Binary Sequence
  PRBS_20 = 'prbs_20',       // 2^20-1 Pseudo Random Binary Sequence
  PRBS_23 = 'prbs_23',       // 2^23-1 Pseudo Random Binary Sequence
  PRBS_31 = 'prbs_31',       // 2^31-1 Pseudo Random Binary Sequence
  ALL_ONES = 'all_ones',     // All 1s pattern
  ALL_ZEROS = 'all_zeros',   // All 0s pattern
  ALTERNATING = 'alternating', // Alternating 1010... pattern
  QRSS = 'qrss',            // Quasi Random Signal Source
  USER_DEFINED = 'user_defined'
}

export interface BERTTest {
  id: string;
  channelId: number;
  pattern: BERTPattern;
  status: 'idle' | 'running' | 'paused' | 'completed' | 'failed';
  startTime?: Date;
  endTime?: Date;
  duration: number; // Test duration in seconds
  elapsedTime: number;
  bitsTransmitted: number;
  bitsReceived: number;
  errorBits: number;
  errorRate: number; // Bit Error Rate (BER)
  errorSeconds: number; // Number of seconds with errors
  severelyErroredSeconds: number; // SES - seconds with >30% error rate
  unavailableSeconds: number; // UAS - seconds when test was unavailable
  syncLoss: number; // Number of sync losses
  patternLoss: number; // Number of pattern losses
  lastSyncTime?: Date;
  configuration: BERTConfiguration;
  results: BERTResults;
}

export interface BERTConfiguration {
  pattern: BERTPattern;
  duration: number;
  errorThreshold: number; // BER threshold for alarm
  syncTimeout: number; // Seconds to wait for sync
  userPattern?: Buffer; // For USER_DEFINED pattern
  insertErrors: boolean; // Insert intentional errors for testing
  errorRate?: number; // Rate of intentional errors
}

export interface BERTResults {
  totalBits: number;
  errorBits: number;
  ber: number;
  erroredSeconds: number;
  severelyErroredSeconds: number;
  unavailableSeconds: number;
  syncLosses: number;
  patternLosses: number;
  averageErrorRate: number;
  peakErrorRate: number;
  minErrorRate: number;
}

export class BERTTester extends EventEmitter {
  private tests: Map<string, BERTTest> = new Map();
  private logger: Logger;
  private nextTestId = 1;
  private testIntervals: Map<string, NodeJS.Timeout> = new Map();
  private patternGenerators: Map<BERTPattern, () => Buffer> = new Map();

  constructor(logger: Logger) {
    super();
    this.logger = logger;
    this.initializePatternGenerators();
  }

  private initializePatternGenerators(): void {
    this.patternGenerators.set(BERTPattern.PRBS_9, () => this.generatePRBS(9));
    this.patternGenerators.set(BERTPattern.PRBS_15, () => this.generatePRBS(15));
    this.patternGenerators.set(BERTPattern.PRBS_20, () => this.generatePRBS(20));
    this.patternGenerators.set(BERTPattern.PRBS_23, () => this.generatePRBS(23));
    this.patternGenerators.set(BERTPattern.PRBS_31, () => this.generatePRBS(31));
    this.patternGenerators.set(BERTPattern.ALL_ONES, () => Buffer.alloc(1024, 0xFF));
    this.patternGenerators.set(BERTPattern.ALL_ZEROS, () => Buffer.alloc(1024, 0x00));
    this.patternGenerators.set(BERTPattern.ALTERNATING, () => this.generateAlternating());
    this.patternGenerators.set(BERTPattern.QRSS, () => this.generateQRSS());
  }

  // Start a BERT test
  startBERTTest(channelId: number, config: BERTConfiguration): string {
    const existingTest = this.findActiveTestForChannel(channelId);
    if (existingTest) {
      throw new Error(`Channel ${channelId} already has an active BERT test`);
    }

    const testId = this.generateTestId();
    const test: BERTTest = {
      id: testId,
      channelId,
      pattern: config.pattern,
      status: 'running',
      startTime: new Date(),
      duration: config.duration,
      elapsedTime: 0,
      bitsTransmitted: 0,
      bitsReceived: 0,
      errorBits: 0,
      errorRate: 0,
      errorSeconds: 0,
      severelyErroredSeconds: 0,
      unavailableSeconds: 0,
      syncLoss: 0,
      patternLoss: 0,
      configuration: config,
      results: {
        totalBits: 0,
        errorBits: 0,
        ber: 0,
        erroredSeconds: 0,
        severelyErroredSeconds: 0,
        unavailableSeconds: 0,
        syncLosses: 0,
        patternLosses: 0,
        averageErrorRate: 0,
        peakErrorRate: 0,
        minErrorRate: Number.MAX_VALUE
      }
    };

    this.tests.set(testId, test);
    this.logger.info(`Starting BERT test on channel ${channelId}`, {
      testId,
      pattern: config.pattern,
      duration: config.duration
    });

    // Start transmitting test pattern
    this.startPatternTransmission(testId);

    // Start test monitoring
    this.startTestMonitoring(testId);

    this.emit('testStarted', test);
    return testId;
  }

  // Stop a BERT test
  stopBERTTest(testId: string): boolean {
    const test = this.tests.get(testId);
    if (!test) {
      return false;
    }

    if (test.status === 'idle' || test.status === 'completed') {
      return true;
    }

    test.status = 'completed';
    test.endTime = new Date();

    this.stopPatternTransmission(testId);
    this.stopTestMonitoring(testId);
    this.calculateFinalResults(test);

    this.logger.info(`BERT test ${testId} completed`, {
      channelId: test.channelId,
      duration: test.elapsedTime,
      ber: test.errorRate,
      errorBits: test.errorBits
    });

    this.emit('testCompleted', test);
    return true;
  }

  // Pause a BERT test
  pauseBERTTest(testId: string): boolean {
    const test = this.tests.get(testId);
    if (!test || test.status !== 'running') {
      return false;
    }

    test.status = 'paused';
    this.stopPatternTransmission(testId);
    this.emit('testPaused', test);
    return true;
  }

  // Resume a BERT test
  resumeBERTTest(testId: string): boolean {
    const test = this.tests.get(testId);
    if (!test || test.status !== 'paused') {
      return false;
    }

    test.status = 'running';
    this.startPatternTransmission(testId);
    this.emit('testResumed', test);
    return true;
  }

  // Start pattern transmission
  private startPatternTransmission(testId: string): void {
    const test = this.tests.get(testId);
    if (!test) return;

    const generator = this.patternGenerators.get(test.pattern);
    if (!generator) {
      test.status = 'failed';
      test.results.unavailableSeconds++;
      this.emit('testFailed', test, 'Unknown pattern type');
      return;
    }

    // Emit pattern data to TDM interface
    const interval = setInterval(() => {
      if (test.status !== 'running') {
        clearInterval(interval);
        return;
      }

      const pattern = generator();
      this.emit('transmitPattern', test.channelId, pattern);
      test.bitsTransmitted += pattern.length * 8;

      // Insert intentional errors if configured
      if (test.configuration.insertErrors && test.configuration.errorRate) {
        this.insertIntentionalErrors(pattern, test.configuration.errorRate);
      }
    }, 100); // Send pattern every 100ms
  }

  // Stop pattern transmission
  private stopPatternTransmission(testId: string): void {
    // Pattern transmission is stopped by the interval check in startPatternTransmission
    this.emit('stopPattern', testId);
  }

  // Start test monitoring
  private startTestMonitoring(testId: string): void {
    const interval = setInterval(() => {
      this.updateTestStatistics(testId);
    }, 1000); // Update every second

    this.testIntervals.set(testId, interval);
  }

  // Stop test monitoring
  private stopTestMonitoring(testId: string): void {
    const interval = this.testIntervals.get(testId);
    if (interval) {
      clearInterval(interval);
      this.testIntervals.delete(testId);
    }
  }

  // Update test statistics
  private updateTestStatistics(testId: string): void {
    const test = this.tests.get(testId);
    if (!test || test.status !== 'running') {
      return;
    }

    test.elapsedTime++;

    // Check if test duration exceeded
    if (test.elapsedTime >= test.duration) {
      this.stopBERTTest(testId);
      return;
    }

    // Calculate current error rate
    if (test.bitsReceived > 0) {
      test.errorRate = test.errorBits / test.bitsReceived;
    }

    // Update error seconds counters
    if (test.errorRate > 0) {
      test.errorSeconds++;
    }

    if (test.errorRate > 0.3) { // 30% error rate
      test.severelyErroredSeconds++;
    }

    // Update results
    this.updateTestResults(test);

    // Check alarm thresholds
    if (test.errorRate > test.configuration.errorThreshold) {
      this.emit('bertAlarm', test, 'High error rate detected');
    }

    this.emit('testUpdated', test);
  }

  // Process received pattern data
  processReceivedPattern(channelId: number, data: Buffer): void {
    const test = this.findActiveTestForChannel(channelId);
    if (!test || test.status !== 'running') {
      return;
    }

    test.bitsReceived += data.length * 8;

    // Compare received data with expected pattern
    const errors = this.comparePattern(test, data);
    test.errorBits += errors;

    // Update sync status
    this.updateSyncStatus(test, data);
  }

  // Compare received pattern with expected pattern
  private comparePattern(test: BERTTest, receivedData: Buffer): number {
    // Generate expected pattern
    const generator = this.patternGenerators.get(test.pattern);
    if (!generator) {
      return 0;
    }

    let errors = 0;
    const expectedData = generator();

    // Compare bit by bit (simplified)
    const minLength = Math.min(receivedData.length, expectedData.length);
    for (let i = 0; i < minLength; i++) {
      const receivedByte = receivedData[i];
      const expectedByte = expectedData[i];
      
      // XOR to find differing bits
      const diff = receivedByte ^ expectedByte;
      
      // Count set bits in diff
      errors += this.countSetBits(diff);
    }

    return errors;
  }

  // Count set bits in a byte
  private countSetBits(byte: number): number {
    let count = 0;
    while (byte) {
      count += byte & 1;
      byte >>= 1;
    }
    return count;
  }

  // Update sync status
  private updateSyncStatus(test: BERTTest, data: Buffer): void {
    // Simplified sync detection
    const syncPattern = this.getSyncPattern(test.pattern);
    const hasSyncPattern = this.findSyncPattern(data, syncPattern);
    
    if (!hasSyncPattern) {
      test.syncLoss++;
      if (!test.lastSyncTime || (Date.now() - test.lastSyncTime.getTime()) > 5000) {
        test.patternLoss++;
      }
    } else {
      test.lastSyncTime = new Date();
    }
  }

  // Generate PRBS pattern
  private generatePRBS(order: number): Buffer {
    const length = 1024; // Generate 1KB of pattern
    const buffer = Buffer.alloc(length);
    
    let shift_register = 1;
    const polynomial = this.getPRBSPolynomial(order);
    
    for (let i = 0; i < length; i++) {
      let byte = 0;
      for (let bit = 0; bit < 8; bit++) {
        const output_bit = shift_register & 1;
        byte = (byte << 1) | output_bit;
        
        const feedback = this.calculatePRBSFeedback(shift_register, polynomial);
        shift_register = (shift_register >> 1) | (feedback << (order - 1));
      }
      buffer[i] = byte;
    }
    
    return buffer;
  }

  // Get PRBS polynomial
  private getPRBSPolynomial(order: number): number {
    const polynomials: { [key: number]: number } = {
      9: 0x11,   // x^9 + x^4 + 1
      15: 0x6001, // x^15 + x^14 + 1
      20: 0x90000, // x^20 + x^19 + 1
      23: 0x420000, // x^23 + x^22 + 1
      31: 0x48000000 // x^31 + x^30 + 1
    };
    return polynomials[order] || 0x11;
  }

  // Calculate PRBS feedback
  private calculatePRBSFeedback(register: number, polynomial: number): number {
    let feedback = 0;
    let temp = register & polynomial;
    
    while (temp) {
      feedback ^= temp & 1;
      temp >>= 1;
    }
    
    return feedback;
  }

  // Generate alternating pattern
  private generateAlternating(): Buffer {
    const buffer = Buffer.alloc(1024);
    for (let i = 0; i < buffer.length; i++) {
      buffer[i] = i % 2 === 0 ? 0xAA : 0x55; // 10101010 : 01010101
    }
    return buffer;
  }

  // Generate QRSS pattern
  private generateQRSS(): Buffer {
    // Simplified QRSS - in reality this would be more complex
    return this.generatePRBS(20);
  }

  // Get sync pattern for a given test pattern
  private getSyncPattern(pattern: BERTPattern): Buffer {
    // Simplified sync pattern detection
    return Buffer.from([0x7E]); // Flag pattern
  }

  // Find sync pattern in data
  private findSyncPattern(data: Buffer, syncPattern: Buffer): boolean {
    return data.indexOf(syncPattern) !== -1;
  }

  // Insert intentional errors
  private insertIntentionalErrors(data: Buffer, errorRate: number): void {
    const totalBits = data.length * 8;
    const errorsToInsert = Math.floor(totalBits * errorRate);
    
    for (let i = 0; i < errorsToInsert; i++) {
      const byteIndex = Math.floor(Math.random() * data.length);
      const bitIndex = Math.floor(Math.random() * 8);
      data[byteIndex] ^= (1 << bitIndex); // Flip the bit
    }
  }

  // Update test results
  private updateTestResults(test: BERTTest): void {
    test.results.totalBits = test.bitsReceived;
    test.results.errorBits = test.errorBits;
    test.results.ber = test.errorRate;
    test.results.erroredSeconds = test.errorSeconds;
    test.results.severelyErroredSeconds = test.severelyErroredSeconds;
    test.results.unavailableSeconds = test.unavailableSeconds;
    test.results.syncLosses = test.syncLoss;
    test.results.patternLosses = test.patternLoss;
    
    // Update min/max/average error rates
    if (test.errorRate > test.results.peakErrorRate) {
      test.results.peakErrorRate = test.errorRate;
    }
    
    if (test.errorRate < test.results.minErrorRate && test.errorRate > 0) {
      test.results.minErrorRate = test.errorRate;
    }
    
    if (test.elapsedTime > 0) {
      test.results.averageErrorRate = test.errorBits / test.bitsReceived;
    }
  }

  // Calculate final results
  private calculateFinalResults(test: BERTTest): void {
    this.updateTestResults(test);
    
    // Final calculations
    test.results.ber = test.bitsReceived > 0 ? test.errorBits / test.bitsReceived : 0;
    
    this.logger.info(`BERT test ${test.id} final results`, {
      ber: test.results.ber,
      errorBits: test.results.errorBits,
      totalBits: test.results.totalBits,
      erroredSeconds: test.results.erroredSeconds
    });
  }

  // Find active test for channel
  private findActiveTestForChannel(channelId: number): BERTTest | undefined {
    for (const test of this.tests.values()) {
      if (test.channelId === channelId && 
          (test.status === 'running' || test.status === 'paused')) {
        return test;
      }
    }
    return undefined;
  }

  // Utility methods
  getTest(testId: string): BERTTest | undefined {
    return this.tests.get(testId);
  }

  getAllTests(): BERTTest[] {
    return Array.from(this.tests.values());
  }

  getActiveTests(): BERTTest[] {
    return Array.from(this.tests.values()).filter(test => 
      test.status === 'running' || test.status === 'paused'
    );
  }

  private generateTestId(): string {
    return `BERT-${Date.now()}-${this.nextTestId++}`;
  }

  // Export test results
  exportTestResults(): any[] {
    return this.getAllTests().map(test => ({
      id: test.id,
      channelId: test.channelId,
      pattern: test.pattern,
      status: test.status,
      startTime: test.startTime?.toISOString(),
      endTime: test.endTime?.toISOString(),
      duration: test.duration,
      elapsedTime: test.elapsedTime,
      results: test.results,
      configuration: test.configuration
    }));
  }
}