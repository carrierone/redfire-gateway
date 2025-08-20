import { EventEmitter } from 'events';
import * as os from 'os';
import { Logger } from '../utils/logger';
import { GatewayConfig } from '../types';
import { TDMoEInterface } from '../interfaces/tdmoe';
import { FreeTDMInterface } from '../interfaces/freetdm';
import { SIPHandler } from '../protocols/sip';
import { RTPHandler } from '../protocols/rtp';
import { PRIEmulator } from '../protocols/pri';
import { SigtranISUPHandler } from '../protocols/sigtran';
import { ProtocolTranslator } from './protocol-translator';
import { AlarmManager } from '../alarms';
import { SNMPService } from '../snmp';
import { CLIService } from '../cli';
import { LoopbackTester } from '../testing/loopback';
import { BERTTester } from '../testing/bert';
import { PerformanceMonitor } from '../services/performance-monitor';

export class RedFireGateway extends EventEmitter {
  private config: GatewayConfig;
  private logger: Logger;
  private isRunning = false;

  // Core interfaces
  private tdmoeInterface?: TDMoEInterface;
  private freetdmInterface?: FreeTDMInterface;
  
  // Protocol handlers
  private sipHandler?: SIPHandler;
  private rtpHandler?: RTPHandler;
  private priEmulator?: PRIEmulator;
  private sigtranHandler?: SigtranISUPHandler;
  private protocolTranslator?: ProtocolTranslator;
  
  // Services
  private alarmManager?: AlarmManager;
  private snmpService?: SNMPService;
  private cliService?: CLIService;
  
  // Testing services
  private loopbackTester?: LoopbackTester;
  private bertTester?: BERTTester;
  
  // Performance monitoring
  private performanceMonitor?: PerformanceMonitor;

  constructor(config: GatewayConfig, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger;
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('Gateway is already running');
    }

    try {
      this.logger.info('Initializing Redfire Gateway components');
      
      // Initialize core services
      await this.initializeServices();
      
      // Initialize interfaces
      await this.initializeInterfaces();
      
      // Initialize protocol handlers
      await this.initializeProtocolHandlers();
      
      // Initialize testing services
      await this.initializeTestingServices();
      
      // Set up inter-component communication
      this.setupEventHandlers();
      
      // Start all components
      await this.startComponents();
      
      this.isRunning = true;
      this.logger.info('Redfire Gateway started successfully');
      this.emit('started');
      
    } catch (error) {
      this.logger.error('Failed to start gateway', error);
      await this.cleanup();
      throw error;
    }
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.logger.info('Stopping Redfire Gateway');
    
    try {
      await this.stopComponents();
      await this.cleanup();
      
      this.isRunning = false;
      this.logger.info('Redfire Gateway stopped');
      this.emit('stopped');
      
    } catch (error) {
      this.logger.error('Error during gateway shutdown', error);
      throw error;
    }
  }

  private async initializeServices(): Promise<void> {
    // Initialize alarm manager
    this.alarmManager = new AlarmManager(this.logger.child({ component: 'alarm-manager' }));
    
    // Initialize SNMP service
    this.snmpService = new SNMPService(
      {
        community: 'public',
        port: 161,
        bindAddress: '0.0.0.0',
        version: 1
      },
      this.logger.child({ component: 'snmp' })
    );
    
    // Initialize CLI service
    this.cliService = new CLIService(this.logger.child({ component: 'cli' }));
    
    // Initialize performance monitor
    this.performanceMonitor = new PerformanceMonitor(
      this.logger.child({ component: 'performance' }),
      {
        cpu: { warning: 80, critical: 95 },
        memory: { warning: 80, critical: 95 },
        disk: { warning: 90, critical: 98 },
        load: { warning: os.cpus().length * 0.8, critical: os.cpus().length * 1.5 },
        network: { errorRate: 0.1, utilizationWarning: 80 }
      }
    );
    
    this.logger.debug('Core services initialized');
  }

  private async initializeInterfaces(): Promise<void> {
    // Initialize TDMoE interface
    this.tdmoeInterface = new TDMoEInterface(
      this.config.tdmoe.interface,
      this.config.tdmoe.channels
    );
    
    // Initialize FreeTDM interface if enabled
    if (this.config.freetdm.enabled) {
      this.freetdmInterface = new FreeTDMInterface(
        this.config.freetdm,
        this.logger.child({ component: 'freetdm' })
      );
    }
    
    this.logger.debug('Interfaces initialized');
  }

  private async initializeProtocolHandlers(): Promise<void> {
    // Initialize SIP handler
    this.sipHandler = new SIPHandler(
      this.config.sip.listenPort,
      this.config.sip.domain,
      this.config.sip.transport
    );
    
    // Initialize RTP handler
    this.rtpHandler = new RTPHandler(this.config.rtp.portRange);
    
    // Initialize PRI emulator
    this.priEmulator = new PRIEmulator(
      this.config.pri.variant,
      this.config.pri.switchType
    );
    
    // Initialize Sigtran ISUP handler if enabled
    if (this.config.sigtran.enabled) {
      this.sigtranHandler = new SigtranISUPHandler({
        localPointCode: this.config.sigtran.pointCodes.local,
        remotePointCode: this.config.sigtran.pointCodes.remote,
        networkIndicator: 2,
        serviceIndicator: 5,
        variant: this.config.sigtran.variant,
        applicationServer: 'localhost',
        routingKey: 1,
        trafficMode: 'override'
      });
    }
    
    // Initialize protocol translator
    this.protocolTranslator = new ProtocolTranslator(
      this.logger.child({ component: 'translator' })
    );
    
    this.logger.debug('Protocol handlers initialized');
  }

  private async initializeTestingServices(): Promise<void> {
    // Initialize loopback tester
    this.loopbackTester = new LoopbackTester(
      this.logger.child({ component: 'loopback-tester' })
    );
    
    // Initialize BERT tester
    this.bertTester = new BERTTester(
      this.logger.child({ component: 'bert-tester' })
    );
    
    this.logger.debug('Testing services initialized');
  }

  private setupEventHandlers(): void {
    // TDMoE Interface events
    if (this.tdmoeInterface) {
      this.tdmoeInterface.on('frameIn', (frame) => {
        this.handleTDMFrame(frame);
      });
      
      this.tdmoeInterface.on('remoteLoopResponse', (response) => {
        this.loopbackTester?.handleTDMLoopbackResponse(
          response.channel,
          response.success
        );
      });
    }
    
    // FreeTDM Interface events
    if (this.freetdmInterface) {
      this.freetdmInterface.on('incomingCall', (callInfo) => {
        this.handleFreeTDMIncomingCall(callInfo);
      });
      
      this.freetdmInterface.on('alarm', (alarm) => {
        this.alarmManager?.raiseAlarm(
          'interface_down' as any,
          'major' as any,
          alarm.message,
          `freetdm.span.${alarm.spanId}`
        );
      });
    }
    
    // SIP Handler events
    if (this.sipHandler) {
      this.sipHandler.on('incomingCall', (session, message) => {
        this.handleSIPIncomingCall(session, message);
      });
      
      this.sipHandler.on('sessionTerminated', (session) => {
        this.handleSIPSessionTerminated(session);
      });
    }
    
    // Alarm Manager events
    if (this.alarmManager) {
      this.alarmManager.on('alarmRaised', (alarm) => {
        this.snmpService?.sendAlarmTrap(
          alarm.type,
          alarm.severity,
          alarm.message
        );
      });
    }
    
    // CLI Service events
    if (this.cliService) {
      this.cliService.on('showInterfaces', () => {
        this.handleCLIShowInterfaces();
      });
      
      this.cliService.on('loopbackAction', (action, channel) => {
        this.handleCLILoopbackAction(action, channel);
      });
      
      this.cliService.on('bertAction', (action, channel, pattern) => {
        this.handleCLIBertAction(action, channel, pattern);
      });
    }
    
    // SNMP Service events
    if (this.snmpService) {
      this.snmpService.on('requestActiveChannelCount', () => {
        const count = this.tdmoeInterface?.getChannelStatus() || new Map();
        const activeCount = Array.from(count.values()).filter(Boolean).length;
        this.snmpService?.updateMIBValue('1.3.6.1.4.1.12345.1.1.2.0', activeCount);
      });
    }
    
    // Performance Monitor events
    if (this.performanceMonitor) {
      this.performanceMonitor.on('alert', (alert) => {
        this.logger.warn('Performance alert', alert);
        
        // Raise alarm for critical performance issues
        if (alert.level === 'critical') {
          this.alarmManager?.raiseAlarm(
            'performance_critical' as any,
            'critical' as any,
            alert.message,
            'system.performance'
          );
        }
        
        // Send SNMP performance trap for alerts
        this.snmpService?.sendPerformanceTrap(
          alert.type,
          alert.value,
          alert.threshold,
          alert.level === 'critical' ? 'critical' : 'warning'
        );
      });
      
      this.performanceMonitor.on('metrics', (metrics) => {
        // Update gateway metrics for SNMP
        this.updateGatewayMetrics();
        
        // Update SNMP MIB values with performance data
        const snmpValues = this.performanceMonitor?.getSNMPValues();
        if (snmpValues && this.snmpService) {
          this.snmpService.updateMIBValue('1.3.6.1.4.1.12345.1.2.1.0', snmpValues.cpuUsage);
          this.snmpService.updateMIBValue('1.3.6.1.4.1.12345.1.2.2.0', snmpValues.memoryUsage);
          this.snmpService.updateMIBValue('1.3.6.1.4.1.12345.1.2.3.0', snmpValues.loadAverage);
          this.snmpService.updateMIBValue('1.3.6.1.4.1.12345.1.2.4.0', snmpValues.activeCalls);
          this.snmpService.updateMIBValue('1.3.6.1.4.1.12345.1.2.5.0', snmpValues.activeChannels);
        }
      });
    }
    
    this.logger.debug('Event handlers set up');
  }

  private async startComponents(): Promise<void> {
    const startPromises: Promise<void>[] = [];
    
    // Start interfaces
    if (this.tdmoeInterface) {
      startPromises.push(this.tdmoeInterface.start());
    }
    
    if (this.freetdmInterface) {
      startPromises.push(this.freetdmInterface.start());
    }
    
    // Start protocol handlers
    if (this.sipHandler) {
      startPromises.push(this.sipHandler.start());
    }
    
    if (this.priEmulator) {
      startPromises.push(this.priEmulator.start());
    }
    
    if (this.sigtranHandler) {
      startPromises.push(this.sigtranHandler.start());
    }
    
    // Start services
    if (this.snmpService) {
      startPromises.push(this.snmpService.start());
    }
    
    if (this.cliService) {
      startPromises.push(this.cliService.start());
    }
    
    if (this.performanceMonitor) {
      startPromises.push(this.performanceMonitor.start(5000)); // 5-second interval
    }
    
    await Promise.all(startPromises);
    this.logger.debug('All components started');
  }

  private async stopComponents(): Promise<void> {
    const stopPromises: Promise<void>[] = [];
    
    // Stop services first
    if (this.performanceMonitor) {
      stopPromises.push(this.performanceMonitor.stop());
    }
    
    if (this.cliService) {
      this.cliService.stop();
    }
    
    if (this.snmpService) {
      stopPromises.push(this.snmpService.stop());
    }
    
    // Stop protocol handlers
    if (this.sigtranHandler) {
      stopPromises.push(this.sigtranHandler.stop());
    }
    
    if (this.priEmulator) {
      stopPromises.push(this.priEmulator.stop());
    }
    
    if (this.sipHandler) {
      stopPromises.push(this.sipHandler.stop());
    }
    
    // Stop interfaces
    if (this.freetdmInterface) {
      stopPromises.push(this.freetdmInterface.stop());
    }
    
    if (this.tdmoeInterface) {
      stopPromises.push(this.tdmoeInterface.stop());
    }
    
    await Promise.all(stopPromises);
    this.logger.debug('All components stopped');
  }

  private async cleanup(): Promise<void> {
    // Cleanup any remaining resources
    this.removeAllListeners();
    this.logger.debug('Gateway cleanup completed');
  }

  // Update gateway metrics for performance monitoring
  private updateGatewayMetrics(): void {
    if (!this.performanceMonitor) return;
    
    // Count active calls across all protocol handlers
    let activeCalls = 0;
    let activeChannels = 0;
    
    if (this.sipHandler) {
      activeCalls += this.sipHandler.getAllSessions().filter(s => 
        s.state === 'connected' || s.state === 'alerting'
      ).length;
    }
    
    if (this.tdmoeInterface) {
      const channelStatus = this.tdmoeInterface.getChannelStatus();
      activeChannels = Array.from(channelStatus.values()).filter(Boolean).length;
    }
    
    if (this.freetdmInterface) {
      const spanStatuses = this.freetdmInterface.getAllSpanStatuses();
      for (const spanStatus of spanStatuses) {
        activeChannels += spanStatus.channels?.filter(ch => ch.state === 'in_use').length || 0;
      }
    }
    
    // Update performance monitor with gateway-specific metrics
    this.performanceMonitor.updateGatewayMetrics({
      activeCalls,
      activeChannels,
      // These would be calculated from actual packet counters in a real implementation
      packetsPerSecond: 0,
      errorsPerSecond: 0,
      alarmCount: this.alarmManager?.getActiveAlarms().length || 0
    });
  }

  // Event handlers
  private handleTDMFrame(frame: any): void {
    this.logger.trace('Received TDM frame', { channel: frame.channel, length: frame.data.length });
    // Process TDM frame and route to appropriate protocol handler
  }

  private handleFreeTDMIncomingCall(callInfo: any): void {
    this.logger.info('FreeTDM incoming call', callInfo);
    // Convert FreeTDM call to SIP session
  }

  private handleSIPIncomingCall(session: any, message: any): void {
    this.logger.info('SIP incoming call', { sessionId: session.id });
    // Convert SIP call to TDM/PRI call
  }

  private handleSIPSessionTerminated(session: any): void {
    this.logger.info('SIP session terminated', { sessionId: session.id });
    // Clean up associated TDM resources
  }

  private handleCLIShowInterfaces(): void {
    const interfaces = [];
    
    if (this.tdmoeInterface) {
      interfaces.push({
        name: 'TDMoE',
        status: 'up',
        channels: this.tdmoeInterface.getChannelStatus()
      });
    }
    
    if (this.freetdmInterface) {
      interfaces.push({
        name: 'FreeTDM',
        status: this.freetdmInterface.isRunning() ? 'up' : 'down',
        spans: this.freetdmInterface.getAllSpanStatuses()
      });
    }
    
    console.log('Interface Status:');
    interfaces.forEach(iface => {
      console.log(`  ${iface.name}: ${iface.status}`);
    });
  }

  private handleCLILoopbackAction(action: string, channel?: number): void {
    if (!this.loopbackTester || !channel) {
      console.log('Loopback tester not available or channel not specified');
      return;
    }
    
    switch (action) {
      case 'start':
        try {
          const testId = this.loopbackTester.startLoopbackTest(channel, 'local' as any);
          console.log(`Started loopback test ${testId} on channel ${channel}`);
        } catch (error) {
          console.log(`Failed to start loopback test: ${error}`);
        }
        break;
      case 'stop':
        const activeTests = this.loopbackTester.getActiveTests();
        const channelTest = activeTests.find(test => test.channelId === channel);
        if (channelTest) {
          this.loopbackTester.stopLoopbackTest(channelTest.id);
          console.log(`Stopped loopback test on channel ${channel}`);
        } else {
          console.log(`No active loopback test on channel ${channel}`);
        }
        break;
      case 'status':
        const tests = this.loopbackTester.getTestsForChannel(channel);
        console.log(`Loopback tests for channel ${channel}:`, tests);
        break;
    }
  }

  private handleCLIBertAction(action: string, channel?: number, pattern?: string): void {
    if (!this.bertTester || !channel) {
      console.log('BERT tester not available or channel not specified');
      return;
    }
    
    switch (action) {
      case 'start':
        try {
          const testId = this.bertTester.startBERTTest(channel, {
            pattern: (pattern as any) || 'prbs_23',
            duration: 60,
            errorThreshold: 0.001,
            syncTimeout: 10,
            insertErrors: false
          });
          console.log(`Started BERT test ${testId} on channel ${channel}`);
        } catch (error) {
          console.log(`Failed to start BERT test: ${error}`);
        }
        break;
      case 'stop':
        const activeTests = this.bertTester.getActiveTests();
        const channelTest = activeTests.find(test => test.channelId === channel);
        if (channelTest) {
          this.bertTester.stopBERTTest(channelTest.id);
          console.log(`Stopped BERT test on channel ${channel}`);
        } else {
          console.log(`No active BERT test on channel ${channel}`);
        }
        break;
      case 'status':
        const test = this.bertTester.getActiveTests().find(t => t.channelId === channel);
        if (test) {
          console.log(`BERT test status for channel ${channel}:`, {
            status: test.status,
            errorRate: test.errorRate,
            bitsTransmitted: test.bitsTransmitted,
            errorBits: test.errorBits
          });
        } else {
          console.log(`No active BERT test on channel ${channel}`);
        }
        break;
      case 'results':
        const allTests = this.bertTester.getAllTests().filter(t => t.channelId === channel);
        console.log(`BERT test results for channel ${channel}:`, allTests.map(t => t.results));
        break;
    }
  }

  // Public API methods
  getStatus(): any {
    return {
      running: this.isRunning,
      interfaces: {
        tdmoe: this.tdmoeInterface ? 'running' : 'disabled',
        freetdm: this.freetdmInterface?.isRunning() ? 'running' : 'disabled'
      },
      protocols: {
        sip: this.sipHandler ? 'running' : 'disabled',
        pri: this.priEmulator ? 'running' : 'disabled',
        sigtran: this.sigtranHandler ? 'running' : 'disabled'
      },
      services: {
        snmp: this.snmpService?.isServiceRunning() ? 'running' : 'disabled',
        cli: this.cliService ? 'running' : 'disabled'
      },
      testing: {
        loopback: this.loopbackTester?.hasActiveTests() ? 'active' : 'idle',
        bert: this.bertTester?.getActiveTests().length > 0 ? 'active' : 'idle'
      },
      performance: {
        monitoring: this.performanceMonitor?.isRunning() ? 'running' : 'disabled',
        metrics: this.performanceMonitor?.getCurrentMetrics() || null
      }
    };
  }

  getConfiguration(): GatewayConfig {
    return { ...this.config };
  }

  isGatewayRunning(): boolean {
    return this.isRunning;
  }
}