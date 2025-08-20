import { RedFireGateway } from './core/gateway';
import { createLogger, LogLevel } from './utils/logger';
import { defaultConfig } from './config';

const logger = createLogger({
  level: LogLevel.INFO,
  prettyPrint: true
});

async function main(): Promise<void> {
  try {
    logger.info('Starting Redfire Gateway');
    
    const gateway = new RedFireGateway(defaultConfig, logger);
    
    // Handle graceful shutdown
    process.on('SIGINT', async () => {
      logger.info('Received SIGINT, shutting down gracefully');
      await gateway.stop();
      process.exit(0);
    });
    
    process.on('SIGTERM', async () => {
      logger.info('Received SIGTERM, shutting down gracefully');
      await gateway.stop();
      process.exit(0);
    });
    
    await gateway.start();
    logger.info('Redfire Gateway started successfully');
    
  } catch (error) {
    logger.fatal('Failed to start Redfire Gateway', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(error => {
    console.error('Unhandled error:', error);
    process.exit(1);
  });
}