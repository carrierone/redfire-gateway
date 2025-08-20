import { createInterface, Interface } from 'readline';
import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export interface CLICommand {
  name: string;
  description: string;
  handler: (args: string[]) => Promise<string>;
}

export class CLIService extends EventEmitter {
  private rl: Interface;
  private commands: Map<string, CLICommand> = new Map();
  private logger: Logger;
  private isRunning = false;

  constructor(logger: Logger) {
    super();
    this.logger = logger;
    this.rl = createInterface({
      input: process.stdin,
      output: process.stdout,
      prompt: 'redfire-gw> '
    });

    this.setupCommands();
    this.setupEventHandlers();
  }

  private setupCommands(): void {
    this.registerCommand({
      name: 'help',
      description: 'Show available commands',
      handler: async () => this.showHelp()
    });

    this.registerCommand({
      name: 'show',
      description: 'Show system information',
      handler: async (args) => this.handleShow(args)
    });

    this.registerCommand({
      name: 'set',
      description: 'Set configuration parameters',
      handler: async (args) => this.handleSet(args)
    });

    this.registerCommand({
      name: 'test',
      description: 'Run diagnostic tests',
      handler: async (args) => this.handleTest(args)
    });

    this.registerCommand({
      name: 'alarm',
      description: 'Alarm management commands',
      handler: async (args) => this.handleAlarm(args)
    });

    this.registerCommand({
      name: 'loopback',
      description: 'Loopback testing commands',
      handler: async (args) => this.handleLoopback(args)
    });

    this.registerCommand({
      name: 'bert',
      description: 'BERT testing commands',
      handler: async (args) => this.handleBert(args)
    });

    this.registerCommand({
      name: 'status',
      description: 'Show gateway status',
      handler: async () => this.handleStatus()
    });

    this.registerCommand({
      name: 'restart',
      description: 'Restart services',
      handler: async (args) => this.handleRestart(args)
    });

    this.registerCommand({
      name: 'exit',
      description: 'Exit CLI',
      handler: async () => {
        this.stop();
        return 'Goodbye!';
      }
    });
  }

  private setupEventHandlers(): void {
    this.rl.on('line', async (input) => {
      const line = input.trim();
      if (line) {
        await this.executeCommand(line);
      }
      this.rl.prompt();
    });

    this.rl.on('close', () => {
      this.stop();
    });
  }

  registerCommand(command: CLICommand): void {
    this.commands.set(command.name, command);
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      return;
    }

    this.isRunning = true;
    console.log('Redfire Gateway CLI v1.0.0');
    console.log('Type "help" for available commands');
    this.rl.prompt();
    this.emit('started');
  }

  stop(): void {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;
    this.rl.close();
    this.emit('stopped');
  }

  private async executeCommand(input: string): Promise<void> {
    const parts = input.split(' ');
    const commandName = parts[0];
    const args = parts.slice(1);

    const command = this.commands.get(commandName);
    if (!command) {
      console.log(`Unknown command: ${commandName}. Type "help" for available commands.`);
      return;
    }

    try {
      const result = await command.handler(args);
      if (result) {
        console.log(result);
      }
    } catch (error) {
      console.log(`Error executing command: ${error instanceof Error ? error.message : error}`);
      this.logger.error('CLI command error', error);
    }
  }

  private async showHelp(): Promise<string> {
    let help = 'Available commands:\n';
    for (const [name, command] of this.commands) {
      help += `  ${name.padEnd(12)} - ${command.description}\n`;
    }
    return help;
  }

  private async handleShow(args: string[]): Promise<string> {
    if (args.length === 0) {
      return 'Usage: show <interfaces|channels|calls|alarms|config>';
    }

    const target = args[0];
    switch (target) {
      case 'interfaces':
        this.emit('showInterfaces');
        return 'Interface information requested';
      case 'channels':
        this.emit('showChannels');
        return 'Channel information requested';
      case 'calls':
        this.emit('showCalls');
        return 'Active calls information requested';
      case 'alarms':
        this.emit('showAlarms');
        return 'Alarm information requested';
      case 'config':
        this.emit('showConfig');
        return 'Configuration information requested';
      default:
        return `Unknown show target: ${target}`;
    }
  }

  private async handleSet(args: string[]): Promise<string> {
    if (args.length < 2) {
      return 'Usage: set <parameter> <value>';
    }

    const parameter = args[0];
    const value = args.slice(1).join(' ');
    
    this.emit('setConfig', parameter, value);
    return `Setting ${parameter} to ${value}`;
  }

  private async handleTest(args: string[]): Promise<string> {
    if (args.length === 0) {
      return 'Usage: test <loopback|bert|connectivity>';
    }

    const testType = args[0];
    this.emit('runTest', testType, args.slice(1));
    return `Running ${testType} test`;
  }

  private async handleAlarm(args: string[]): Promise<string> {
    if (args.length === 0) {
      return 'Usage: alarm <list|clear|acknowledge> [alarm-id]';
    }

    const action = args[0];
    const alarmId = args[1];
    
    this.emit('alarmAction', action, alarmId);
    return `Alarm ${action} requested`;
  }

  private async handleLoopback(args: string[]): Promise<string> {
    if (args.length === 0) {
      return 'Usage: loopback <start|stop|status> [channel]';
    }

    const action = args[0];
    const channel = args[1] ? parseInt(args[1]) : undefined;
    
    this.emit('loopbackAction', action, channel);
    return `Loopback ${action} requested${channel ? ` for channel ${channel}` : ''}`;
  }

  private async handleBert(args: string[]): Promise<string> {
    if (args.length === 0) {
      return 'Usage: bert <start|stop|status|results> [channel] [pattern]';
    }

    const action = args[0];
    const channel = args[1] ? parseInt(args[1]) : undefined;
    const pattern = args[2];
    
    this.emit('bertAction', action, channel, pattern);
    return `BERT ${action} requested${channel ? ` for channel ${channel}` : ''}`;
  }

  private async handleStatus(): Promise<string> {
    this.emit('getStatus');
    return 'Gateway status requested';
  }

  private async handleRestart(args: string[]): Promise<string> {
    const service = args[0] || 'all';
    this.emit('restart', service);
    return `Restarting ${service} service(s)`;
  }
}