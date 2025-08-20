import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';
import * as os from 'os';
import * as fs from 'fs/promises';
import * as process from 'process';

export interface PerformanceMetrics {
  timestamp: Date;
  cpu: {
    usage: number;        // Percentage (0-100)
    loadAverage: number[];  // 1, 5, 15 minute load averages
    cores: number;
    model: string;
    speed: number;        // MHz
  };
  memory: {
    total: number;        // Bytes
    used: number;         // Bytes
    free: number;         // Bytes
    percentage: number;   // Percentage (0-100)
    buffers?: number;     // Linux specific
    cached?: number;      // Linux specific
  };
  process: {
    pid: number;
    uptime: number;       // Seconds
    heapUsed: number;     // Bytes
    heapTotal: number;    // Bytes
    external: number;     // Bytes
    rss: number;          // Bytes (Resident Set Size)
    cpuUsage: {
      user: number;       // Microseconds
      system: number;     // Microseconds
    };
  };
  network: {
    interfaces: NetworkInterfaceMetrics[];
    connections: {
      established: number;
      listening: number;
      total: number;
    };
  };
  disk: {
    usage: DiskUsageMetrics[];
    ioStats?: DiskIOStats;
  };
  gateway: {
    activeCalls: number;
    activeChannels: number;
    packetsPerSecond: number;
    errorsPerSecond: number;
    alarmCount: number;
  };
}

export interface NetworkInterfaceMetrics {
  name: string;
  bytesReceived: number;
  bytesSent: number;
  packetsReceived: number;
  packetsSent: number;
  errorsReceived: number;
  errorsSent: number;
  droppedReceived: number;
  droppedSent: number;
}

export interface DiskUsageMetrics {
  filesystem: string;
  mountpoint: string;
  total: number;        // Bytes
  used: number;         // Bytes
  available: number;    // Bytes
  percentage: number;   // Percentage (0-100)
}

export interface DiskIOStats {
  readOps: number;
  writeOps: number;
  readBytes: number;
  writeBytes: number;
  readTime: number;     // Milliseconds
  writeTime: number;    // Milliseconds
}

export interface PerformanceThresholds {
  cpu: {
    warning: number;    // 80%
    critical: number;   // 95%
  };
  memory: {
    warning: number;    // 80%
    critical: number;   // 95%
  };
  disk: {
    warning: number;    // 80%
    critical: number;   // 95%
  };
  load: {
    warning: number;    // Number of cores * 0.8
    critical: number;   // Number of cores * 1.5
  };
  network: {
    errorRate: number;  // 0.1% packet error rate
    utilizationWarning: number; // 80% utilization
  };
}

export class PerformanceMonitor extends EventEmitter {
  private logger: Logger;
  private isRunning = false;
  private monitoringInterval?: NodeJS.Timeout;
  private intervalMs: number = 5000; // 5 seconds
  private metricsHistory: PerformanceMetrics[] = [];
  private maxHistorySize = 720; // 1 hour at 5-second intervals
  private thresholds: PerformanceThresholds;
  private previousNetworkStats: Map<string, any> = new Map();
  private previousDiskStats?: DiskIOStats;
  private gatewayMetrics = {
    activeCalls: 0,
    activeChannels: 0,
    packetsPerSecond: 0,
    errorsPerSecond: 0,
    alarmCount: 0
  };

  constructor(logger: Logger, thresholds?: Partial<PerformanceThresholds>) {
    super();
    this.logger = logger.child({ component: 'performance-monitor' });
    
    const cores = os.cpus().length;
    this.thresholds = {
      cpu: { warning: 80, critical: 95 },
      memory: { warning: 80, critical: 95 },
      disk: { warning: 80, critical: 95 },
      load: { warning: cores * 0.8, critical: cores * 1.5 },
      network: { errorRate: 0.1, utilizationWarning: 80 },
      ...thresholds
    };
  }

  async start(intervalMs: number = 5000): Promise<void> {
    if (this.isRunning) {
      throw new Error('Performance monitor already running');
    }

    this.intervalMs = intervalMs;
    this.isRunning = true;

    this.logger.info('Starting performance monitoring', {
      interval: intervalMs,
      thresholds: this.thresholds
    });

    // Initial metrics collection
    await this.collectMetrics();

    // Start periodic collection
    this.monitoringInterval = setInterval(async () => {
      try {
        await this.collectMetrics();
      } catch (error) {
        this.logger.error('Error collecting performance metrics', error);
      }
    }, intervalMs);

    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = undefined;
    }

    this.isRunning = false;
    this.logger.info('Performance monitoring stopped');
    this.emit('stopped');
  }

  private async collectMetrics(): Promise<void> {
    const timestamp = new Date();

    try {
      const metrics: PerformanceMetrics = {
        timestamp,
        cpu: await this.getCPUMetrics(),
        memory: await this.getMemoryMetrics(),
        process: await this.getProcessMetrics(),
        network: await this.getNetworkMetrics(),
        disk: await this.getDiskMetrics(),
        gateway: { ...this.gatewayMetrics }
      };

      // Add to history
      this.metricsHistory.push(metrics);
      
      // Trim history if needed
      if (this.metricsHistory.length > this.maxHistorySize) {
        this.metricsHistory = this.metricsHistory.slice(-this.maxHistorySize);
      }

      // Check thresholds and emit alerts
      this.checkThresholds(metrics);

      // Emit metrics event
      this.emit('metrics', metrics);

      this.logger.trace('Performance metrics collected', {
        cpu: metrics.cpu.usage,
        memory: metrics.memory.percentage,
        activeCalls: metrics.gateway.activeCalls
      });

    } catch (error) {
      this.logger.error('Failed to collect performance metrics', error);
    }
  }

  private async getCPUMetrics(): Promise<any> {
    const cpus = os.cpus();
    const loadAvg = os.loadavg();

    // Calculate CPU usage
    let totalIdle = 0;
    let totalTick = 0;

    for (const cpu of cpus) {
      for (const type in cpu.times) {
        totalTick += cpu.times[type as keyof typeof cpu.times];
      }
      totalIdle += cpu.times.idle;
    }

    const idle = totalIdle / cpus.length;
    const total = totalTick / cpus.length;
    const usage = 100 - Math.floor((idle / total) * 100);

    return {
      usage,
      loadAverage: loadAvg,
      cores: cpus.length,
      model: cpus[0]?.model || 'Unknown',
      speed: cpus[0]?.speed || 0
    };
  }

  private async getMemoryMetrics(): Promise<any> {
    const totalMem = os.totalmem();
    const freeMem = os.freemem();
    const usedMem = totalMem - freeMem;
    const percentage = (usedMem / totalMem) * 100;

    const metrics: any = {
      total: totalMem,
      used: usedMem,
      free: freeMem,
      percentage
    };

    // Linux-specific memory info
    if (os.platform() === 'linux') {
      try {
        const meminfo = await fs.readFile('/proc/meminfo', 'utf8');
        const lines = meminfo.split('\n');
        
        for (const line of lines) {
          if (line.startsWith('Buffers:')) {
            metrics.buffers = parseInt(line.split(/\s+/)[1]) * 1024;
          } else if (line.startsWith('Cached:')) {
            metrics.cached = parseInt(line.split(/\s+/)[1]) * 1024;
          }
        }
      } catch (error) {
        // Ignore if /proc/meminfo is not available
      }
    }

    return metrics;
  }

  private async getProcessMetrics(): Promise<any> {
    const memUsage = process.memoryUsage();
    const cpuUsage = process.cpuUsage();

    return {
      pid: process.pid,
      uptime: process.uptime(),
      heapUsed: memUsage.heapUsed,
      heapTotal: memUsage.heapTotal,
      external: memUsage.external,
      rss: memUsage.rss,
      cpuUsage: {
        user: cpuUsage.user,
        system: cpuUsage.system
      }
    };
  }

  private async getNetworkMetrics(): Promise<any> {
    const interfaces: NetworkInterfaceMetrics[] = [];
    let connections = { established: 0, listening: 0, total: 0 };

    // Network interface statistics (Linux)
    if (os.platform() === 'linux') {
      try {
        const netDev = await fs.readFile('/proc/net/dev', 'utf8');
        const lines = netDev.split('\n').slice(2); // Skip header lines

        for (const line of lines) {
          const parts = line.trim().split(/\s+/);
          if (parts.length >= 17) {
            const name = parts[0].replace(':', '');
            
            // Skip loopback
            if (name === 'lo') continue;

            const current = {
              name,
              bytesReceived: parseInt(parts[1]),
              packetsReceived: parseInt(parts[2]),
              errorsReceived: parseInt(parts[3]),
              droppedReceived: parseInt(parts[4]),
              bytesSent: parseInt(parts[9]),
              packetsSent: parseInt(parts[10]),
              errorsSent: parseInt(parts[11]),
              droppedSent: parseInt(parts[12])
            };

            // Calculate rates if we have previous data
            const previous = this.previousNetworkStats.get(name);
            if (previous) {
              const timeDiff = (Date.now() - previous.timestamp) / 1000;
              current.bytesReceived = (current.bytesReceived - previous.bytesReceived) / timeDiff;
              current.bytesSent = (current.bytesSent - previous.bytesSent) / timeDiff;
            }

            this.previousNetworkStats.set(name, {
              ...current,
              timestamp: Date.now()
            });

            interfaces.push(current);
          }
        }
      } catch (error) {
        this.logger.debug('Could not read network statistics', error);
      }

      // Connection statistics
      try {
        const tcpStats = await fs.readFile('/proc/net/tcp', 'utf8');
        const lines = tcpStats.split('\n').slice(1);
        
        for (const line of lines) {
          if (line.trim()) {
            const parts = line.trim().split(/\s+/);
            const state = parseInt(parts[3], 16);
            
            connections.total++;
            if (state === 1) connections.established++; // ESTABLISHED
            if (state === 10) connections.listening++;  // LISTEN
          }
        }
      } catch (error) {
        this.logger.debug('Could not read connection statistics', error);
      }
    }

    return { interfaces, connections };
  }

  private async getDiskMetrics(): Promise<any> {
    const usage: DiskUsageMetrics[] = [];
    let ioStats: DiskIOStats | undefined;

    // Disk usage statistics
    if (os.platform() === 'linux') {
      try {
        const mounts = await fs.readFile('/proc/mounts', 'utf8');
        const mountLines = mounts.split('\n');
        
        for (const line of mountLines) {
          const parts = line.split(' ');
          if (parts.length >= 2 && parts[1].startsWith('/')) {
            const mountpoint = parts[1];
            
            // Skip virtual filesystems
            if (parts[0].startsWith('/dev/') && !mountpoint.includes('snap')) {
              try {
                const stats = await fs.stat(mountpoint);
                // This is simplified - would use statvfs in real implementation
                usage.push({
                  filesystem: parts[0],
                  mountpoint,
                  total: 0,
                  used: 0,
                  available: 0,
                  percentage: 0
                });
              } catch (error) {
                // Skip if we can't stat the mountpoint
              }
            }
          }
        }

        // Disk I/O statistics
        const diskStats = await fs.readFile('/proc/diskstats', 'utf8');
        const diskLines = diskStats.split('\n');
        
        let totalReadOps = 0, totalWriteOps = 0;
        let totalReadBytes = 0, totalWriteBytes = 0;
        let totalReadTime = 0, totalWriteTime = 0;

        for (const line of diskLines) {
          const parts = line.trim().split(/\s+/);
          if (parts.length >= 14) {
            // Skip partitions, only include whole disks
            if (parts[2].match(/^[sh]d[a-z]$/)) {
              totalReadOps += parseInt(parts[3]);
              totalWriteOps += parseInt(parts[7]);
              totalReadBytes += parseInt(parts[5]) * 512; // Sectors to bytes
              totalWriteBytes += parseInt(parts[9]) * 512;
              totalReadTime += parseInt(parts[6]);
              totalWriteTime += parseInt(parts[10]);
            }
          }
        }

        ioStats = {
          readOps: totalReadOps,
          writeOps: totalWriteOps,
          readBytes: totalReadBytes,
          writeBytes: totalWriteBytes,
          readTime: totalReadTime,
          writeTime: totalWriteTime
        };

        // Calculate rates if we have previous data
        if (this.previousDiskStats) {
          const timeDiff = this.intervalMs / 1000;
          ioStats.readOps = (ioStats.readOps - this.previousDiskStats.readOps) / timeDiff;
          ioStats.writeOps = (ioStats.writeOps - this.previousDiskStats.writeOps) / timeDiff;
          ioStats.readBytes = (ioStats.readBytes - this.previousDiskStats.readBytes) / timeDiff;
          ioStats.writeBytes = (ioStats.writeBytes - this.previousDiskStats.writeBytes) / timeDiff;
        }

        this.previousDiskStats = { ...ioStats };

      } catch (error) {
        this.logger.debug('Could not read disk statistics', error);
      }
    }

    return { usage, ioStats };
  }

  private checkThresholds(metrics: PerformanceMetrics): void {
    const alerts: any[] = [];

    // CPU threshold checks
    if (metrics.cpu.usage >= this.thresholds.cpu.critical) {
      alerts.push({
        type: 'cpu',
        level: 'critical',
        message: `CPU usage is critically high: ${metrics.cpu.usage}%`,
        value: metrics.cpu.usage,
        threshold: this.thresholds.cpu.critical
      });
    } else if (metrics.cpu.usage >= this.thresholds.cpu.warning) {
      alerts.push({
        type: 'cpu',
        level: 'warning',
        message: `CPU usage is high: ${metrics.cpu.usage}%`,
        value: metrics.cpu.usage,
        threshold: this.thresholds.cpu.warning
      });
    }

    // Memory threshold checks
    if (metrics.memory.percentage >= this.thresholds.memory.critical) {
      alerts.push({
        type: 'memory',
        level: 'critical',
        message: `Memory usage is critically high: ${metrics.memory.percentage.toFixed(1)}%`,
        value: metrics.memory.percentage,
        threshold: this.thresholds.memory.critical
      });
    } else if (metrics.memory.percentage >= this.thresholds.memory.warning) {
      alerts.push({
        type: 'memory',
        level: 'warning',
        message: `Memory usage is high: ${metrics.memory.percentage.toFixed(1)}%`,
        value: metrics.memory.percentage,
        threshold: this.thresholds.memory.warning
      });
    }

    // Load average checks
    const loadAvg1Min = metrics.cpu.loadAverage[0];
    if (loadAvg1Min >= this.thresholds.load.critical) {
      alerts.push({
        type: 'load',
        level: 'critical',
        message: `System load is critically high: ${loadAvg1Min.toFixed(2)}`,
        value: loadAvg1Min,
        threshold: this.thresholds.load.critical
      });
    } else if (loadAvg1Min >= this.thresholds.load.warning) {
      alerts.push({
        type: 'load',
        level: 'warning',
        message: `System load is high: ${loadAvg1Min.toFixed(2)}`,
        value: loadAvg1Min,
        threshold: this.thresholds.load.warning
      });
    }

    // Disk usage checks
    for (const disk of metrics.disk.usage) {
      if (disk.percentage >= this.thresholds.disk.critical) {
        alerts.push({
          type: 'disk',
          level: 'critical',
          message: `Disk usage critically high on ${disk.filesystem}: ${disk.percentage.toFixed(1)}%`,
          value: disk.percentage,
          threshold: this.thresholds.disk.critical
        });
      } else if (disk.percentage >= this.thresholds.disk.warning) {
        alerts.push({
          type: 'disk',
          level: 'warning',
          message: `Disk usage high on ${disk.filesystem}: ${disk.percentage.toFixed(1)}%`,
          value: disk.percentage,
          threshold: this.thresholds.disk.warning
        });
      }
    }

    // Network error rate checks
    for (const iface of metrics.network.interfaces) {
      const totalPackets = iface.packetsReceived + iface.packetsSent;
      const totalErrors = iface.errorsReceived + iface.errorsSent;
      
      if (totalPackets > 0) {
        const errorRate = (totalErrors / totalPackets) * 100;
        if (errorRate >= this.thresholds.network.errorRate) {
          alerts.push({
            type: 'network',
            level: 'warning',
            message: `High error rate on ${iface.name}: ${errorRate.toFixed(2)}%`,
            value: errorRate,
            threshold: this.thresholds.network.errorRate
          });
        }
      }
    }

    // Emit alerts
    for (const alert of alerts) {
      this.emit('alert', alert);
      
      if (alert.level === 'critical') {
        this.logger.error('Performance alert', alert);
      } else {
        this.logger.warn('Performance alert', alert);
      }
    }
  }

  // Public API methods
  updateGatewayMetrics(metrics: Partial<typeof this.gatewayMetrics>): void {
    Object.assign(this.gatewayMetrics, metrics);
  }

  getCurrentMetrics(): PerformanceMetrics | null {
    return this.metricsHistory.length > 0 ? 
      this.metricsHistory[this.metricsHistory.length - 1] : null;
  }

  getMetricsHistory(minutes: number = 60): PerformanceMetrics[] {
    const cutoff = new Date(Date.now() - minutes * 60 * 1000);
    return this.metricsHistory.filter(m => m.timestamp >= cutoff);
  }

  getAverageMetrics(minutes: number = 5): any {
    const recent = this.getMetricsHistory(minutes);
    if (recent.length === 0) return null;

    const sums = recent.reduce((acc, metrics) => {
      acc.cpu += metrics.cpu.usage;
      acc.memory += metrics.memory.percentage;
      acc.load += metrics.cpu.loadAverage[0];
      acc.activeCalls += metrics.gateway.activeCalls;
      return acc;
    }, { cpu: 0, memory: 0, load: 0, activeCalls: 0 });

    const count = recent.length;
    return {
      cpu: sums.cpu / count,
      memory: sums.memory / count,
      load: sums.load / count,
      activeCalls: sums.activeCalls / count
    };
  }

  // SNMP integration methods
  getSNMPValues(): any {
    const current = this.getCurrentMetrics();
    if (!current) return {};

    return {
      cpuUsage: Math.round(current.cpu.usage),
      memoryUsage: Math.round(current.memory.percentage),
      loadAverage: Math.round(current.cpu.loadAverage[0] * 100), // Scale for SNMP
      processHeapUsed: current.process.heapUsed,
      processUptime: Math.round(current.process.uptime),
      activeCalls: current.gateway.activeCalls,
      activeChannels: current.gateway.activeChannels,
      packetsPerSecond: Math.round(current.gateway.packetsPerSecond),
      errorsPerSecond: Math.round(current.gateway.errorsPerSecond),
      networkBytesReceived: current.network.interfaces.reduce((sum, iface) => sum + iface.bytesReceived, 0),
      networkBytesSent: current.network.interfaces.reduce((sum, iface) => sum + iface.bytesSent, 0),
      diskReadOps: current.disk.ioStats?.readOps || 0,
      diskWriteOps: current.disk.ioStats?.writeOps || 0
    };
  }

  getThresholds(): PerformanceThresholds {
    return { ...this.thresholds };
  }

  updateThresholds(thresholds: Partial<PerformanceThresholds>): void {
    Object.assign(this.thresholds, thresholds);
    this.logger.info('Performance thresholds updated', thresholds);
  }

  isRunning(): boolean {
    return this.isRunning;
  }

  // Generate performance report
  generateReport(hours: number = 24): any {
    const metrics = this.getMetricsHistory(hours * 60);
    if (metrics.length === 0) return null;

    const cpuValues = metrics.map(m => m.cpu.usage);
    const memoryValues = metrics.map(m => m.memory.percentage);
    const loadValues = metrics.map(m => m.cpu.loadAverage[0]);

    return {
      period: {
        start: metrics[0].timestamp,
        end: metrics[metrics.length - 1].timestamp,
        samples: metrics.length
      },
      cpu: {
        average: cpuValues.reduce((a, b) => a + b, 0) / cpuValues.length,
        min: Math.min(...cpuValues),
        max: Math.max(...cpuValues),
        thresholdBreaches: cpuValues.filter(v => v >= this.thresholds.cpu.warning).length
      },
      memory: {
        average: memoryValues.reduce((a, b) => a + b, 0) / memoryValues.length,
        min: Math.min(...memoryValues),
        max: Math.max(...memoryValues),
        thresholdBreaches: memoryValues.filter(v => v >= this.thresholds.memory.warning).length
      },
      load: {
        average: loadValues.reduce((a, b) => a + b, 0) / loadValues.length,
        min: Math.min(...loadValues),
        max: Math.max(...loadValues),
        thresholdBreaches: loadValues.filter(v => v >= this.thresholds.load.warning).length
      },
      gateway: {
        totalCalls: metrics.reduce((sum, m) => sum + m.gateway.activeCalls, 0),
        peakCalls: Math.max(...metrics.map(m => m.gateway.activeCalls)),
        totalErrors: metrics.reduce((sum, m) => sum + m.gateway.errorsPerSecond, 0) * (hours * 3600)
      }
    };
  }
}