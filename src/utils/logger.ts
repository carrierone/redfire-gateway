import pino from 'pino';

export enum LogLevel {
  TRACE = 'trace',
  DEBUG = 'debug',
  INFO = 'info',
  WARN = 'warn',
  ERROR = 'error',
  FATAL = 'fatal'
}

export interface LoggerConfig {
  level: LogLevel;
  destination?: string;
  prettyPrint?: boolean;
}

export class Logger {
  private logger: pino.Logger;

  constructor(config: LoggerConfig) {
    this.logger = pino({
      level: config.level,
      transport: config.prettyPrint ? {
        target: 'pino-pretty',
        options: {
          colorize: true,
          translateTime: 'yyyy-mm-dd HH:MM:ss',
          ignore: 'pid,hostname'
        }
      } : undefined
    });
  }

  trace(message: string, data?: any): void {
    this.logger.trace(data, message);
  }

  debug(message: string, data?: any): void {
    this.logger.debug(data, message);
  }

  info(message: string, data?: any): void {
    this.logger.info(data, message);
  }

  warn(message: string, data?: any): void {
    this.logger.warn(data, message);
  }

  error(message: string, error?: Error | any): void {
    this.logger.error(error, message);
  }

  fatal(message: string, error?: Error | any): void {
    this.logger.fatal(error, message);
  }

  child(bindings: any): Logger {
    const childLogger = this.logger.child(bindings);
    return {
      trace: (msg, data) => childLogger.trace(data, msg),
      debug: (msg, data) => childLogger.debug(data, msg),
      info: (msg, data) => childLogger.info(data, msg),
      warn: (msg, data) => childLogger.warn(data, msg),
      error: (msg, err) => childLogger.error(err, msg),
      fatal: (msg, err) => childLogger.fatal(err, msg),
      child: (bindings) => this.child(bindings)
    } as Logger;
  }
}

export const createLogger = (config: LoggerConfig): Logger => {
  return new Logger(config);
};