import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export enum AlarmSeverity {
  CRITICAL = 'critical',
  MAJOR = 'major',
  MINOR = 'minor',
  WARNING = 'warning',
  INFO = 'info',
  CLEAR = 'clear'
}

export enum AlarmType {
  INTERFACE_DOWN = 'interface_down',
  CHANNEL_FAILURE = 'channel_failure',
  SIP_SERVICE_DOWN = 'sip_service_down',
  RTP_SESSION_FAILED = 'rtp_session_failed',
  BERT_TEST_FAILED = 'bert_test_failed',
  LOOPBACK_TEST_FAILED = 'loopback_test_failed',
  HIGH_ERROR_RATE = 'high_error_rate',
  SYSTEM_OVERLOAD = 'system_overload',
  CONFIGURATION_ERROR = 'configuration_error',
  AUTHENTICATION_FAILURE = 'authentication_failure'
}

export interface Alarm {
  id: string;
  type: AlarmType;
  severity: AlarmSeverity;
  message: string;
  source: string;
  timestamp: Date;
  acknowledged: boolean;
  cleared: boolean;
  clearTimestamp?: Date;
  acknowledgedBy?: string;
  acknowledgedAt?: Date;
  details?: any;
}

export class AlarmManager extends EventEmitter {
  private alarms: Map<string, Alarm> = new Map();
  private logger: Logger;
  private nextAlarmId = 1;

  constructor(logger: Logger) {
    super();
    this.logger = logger;
  }

  raiseAlarm(
    type: AlarmType, 
    severity: AlarmSeverity, 
    message: string, 
    source: string, 
    details?: any
  ): string {
    const alarm: Alarm = {
      id: this.generateAlarmId(),
      type,
      severity,
      message,
      source,
      timestamp: new Date(),
      acknowledged: false,
      cleared: false,
      details
    };

    this.alarms.set(alarm.id, alarm);
    
    this.logger.warn(`Alarm raised: ${alarm.id}`, {
      type: alarm.type,
      severity: alarm.severity,
      message: alarm.message,
      source: alarm.source
    });

    this.emit('alarmRaised', alarm);
    return alarm.id;
  }

  clearAlarm(alarmId: string): boolean {
    const alarm = this.alarms.get(alarmId);
    if (!alarm) {
      return false;
    }

    if (alarm.cleared) {
      return true;
    }

    alarm.cleared = true;
    alarm.clearTimestamp = new Date();

    this.logger.info(`Alarm cleared: ${alarmId}`, {
      type: alarm.type,
      duration: alarm.clearTimestamp.getTime() - alarm.timestamp.getTime()
    });

    this.emit('alarmCleared', alarm);
    return true;
  }

  acknowledgeAlarm(alarmId: string, acknowledgedBy: string): boolean {
    const alarm = this.alarms.get(alarmId);
    if (!alarm) {
      return false;
    }

    if (alarm.acknowledged) {
      return true;
    }

    alarm.acknowledged = true;
    alarm.acknowledgedBy = acknowledgedBy;
    alarm.acknowledgedAt = new Date();

    this.logger.info(`Alarm acknowledged: ${alarmId} by ${acknowledgedBy}`);
    this.emit('alarmAcknowledged', alarm);
    return true;
  }

  getAlarm(alarmId: string): Alarm | undefined {
    return this.alarms.get(alarmId);
  }

  getAllAlarms(): Alarm[] {
    return Array.from(this.alarms.values());
  }

  getActiveAlarms(): Alarm[] {
    return Array.from(this.alarms.values()).filter(alarm => !alarm.cleared);
  }

  getAlarmsBySeverity(severity: AlarmSeverity): Alarm[] {
    return Array.from(this.alarms.values()).filter(alarm => 
      alarm.severity === severity && !alarm.cleared
    );
  }

  getAlarmsByType(type: AlarmType): Alarm[] {
    return Array.from(this.alarms.values()).filter(alarm => 
      alarm.type === type && !alarm.cleared
    );
  }

  getUnacknowledgedAlarms(): Alarm[] {
    return Array.from(this.alarms.values()).filter(alarm => 
      !alarm.acknowledged && !alarm.cleared
    );
  }

  clearAllAlarmsOfType(type: AlarmType): number {
    let clearedCount = 0;
    for (const alarm of this.alarms.values()) {
      if (alarm.type === type && !alarm.cleared) {
        this.clearAlarm(alarm.id);
        clearedCount++;
      }
    }
    return clearedCount;
  }

  getAlarmCount(): number {
    return this.getActiveAlarms().length;
  }

  getCriticalAlarmCount(): number {
    return this.getAlarmsBySeverity(AlarmSeverity.CRITICAL).length;
  }

  getMajorAlarmCount(): number {
    return this.getAlarmsBySeverity(AlarmSeverity.MAJOR).length;
  }

  getMinorAlarmCount(): number {
    return this.getAlarmsBySeverity(AlarmSeverity.MINOR).length;
  }

  // Predefined alarm raising methods for common scenarios
  raiseInterfaceDownAlarm(interfaceName: string): string {
    return this.raiseAlarm(
      AlarmType.INTERFACE_DOWN,
      AlarmSeverity.CRITICAL,
      `Interface ${interfaceName} is down`,
      `interface.${interfaceName}`,
      { interface: interfaceName }
    );
  }

  raiseChannelFailureAlarm(channelId: number, reason: string): string {
    return this.raiseAlarm(
      AlarmType.CHANNEL_FAILURE,
      AlarmSeverity.MAJOR,
      `Channel ${channelId} failure: ${reason}`,
      `channel.${channelId}`,
      { channel: channelId, reason }
    );
  }

  raiseSIPServiceDownAlarm(): string {
    return this.raiseAlarm(
      AlarmType.SIP_SERVICE_DOWN,
      AlarmSeverity.CRITICAL,
      'SIP service is down',
      'sip.service',
      {}
    );
  }

  raiseHighErrorRateAlarm(source: string, errorRate: number, threshold: number): string {
    return this.raiseAlarm(
      AlarmType.HIGH_ERROR_RATE,
      AlarmSeverity.MAJOR,
      `High error rate detected: ${errorRate}% (threshold: ${threshold}%)`,
      source,
      { errorRate, threshold }
    );
  }

  raiseBertTestFailedAlarm(channelId: number, errorCount: number): string {
    return this.raiseAlarm(
      AlarmType.BERT_TEST_FAILED,
      AlarmSeverity.MINOR,
      `BERT test failed on channel ${channelId} with ${errorCount} errors`,
      `bert.channel.${channelId}`,
      { channel: channelId, errorCount }
    );
  }

  raiseLoopbackTestFailedAlarm(channelId: number, loopType: string): string {
    return this.raiseAlarm(
      AlarmType.LOOPBACK_TEST_FAILED,
      AlarmSeverity.MINOR,
      `Loopback test (${loopType}) failed on channel ${channelId}`,
      `loopback.channel.${channelId}`,
      { channel: channelId, loopType }
    );
  }

  raiseSystemOverloadAlarm(cpuUsage: number, memoryUsage: number): string {
    return this.raiseAlarm(
      AlarmType.SYSTEM_OVERLOAD,
      AlarmSeverity.WARNING,
      `System overload detected - CPU: ${cpuUsage}%, Memory: ${memoryUsage}%`,
      'system.resources',
      { cpuUsage, memoryUsage }
    );
  }

  // Cleanup old cleared alarms
  cleanupOldAlarms(maxAge: number = 7 * 24 * 60 * 60 * 1000): number { // Default 7 days
    const cutoffTime = new Date(Date.now() - maxAge);
    let cleanedCount = 0;

    for (const [id, alarm] of this.alarms) {
      if (alarm.cleared && alarm.clearTimestamp && alarm.clearTimestamp < cutoffTime) {
        this.alarms.delete(id);
        cleanedCount++;
      }
    }

    if (cleanedCount > 0) {
      this.logger.info(`Cleaned up ${cleanedCount} old alarms`);
    }

    return cleanedCount;
  }

  private generateAlarmId(): string {
    return `ALM-${Date.now()}-${this.nextAlarmId++}`;
  }

  // Export alarm data for external systems
  exportAlarms(includeCleared = false): any[] {
    const alarms = includeCleared ? this.getAllAlarms() : this.getActiveAlarms();
    return alarms.map(alarm => ({
      id: alarm.id,
      type: alarm.type,
      severity: alarm.severity,
      message: alarm.message,
      source: alarm.source,
      timestamp: alarm.timestamp.toISOString(),
      acknowledged: alarm.acknowledged,
      cleared: alarm.cleared,
      clearTimestamp: alarm.clearTimestamp?.toISOString(),
      acknowledgedBy: alarm.acknowledgedBy,
      acknowledgedAt: alarm.acknowledgedAt?.toISOString(),
      details: alarm.details
    }));
  }
}