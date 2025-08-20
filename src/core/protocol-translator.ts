import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';
import { SIPMessage, ISUPMessage, PRIMessage, CallSession } from '../types';

export interface ProtocolTranslationRules {
  causeCodeMapping: Map<number, number>;
  progressIndicatorMapping: Map<number, number>;
  numberingPlanMapping: Map<string, number>;
  natureOfAddressMapping: Map<string, number>;
}

export interface TranslationContext {
  sourceProtocol: 'tdm' | 'pri' | 'isup' | 'sip' | 'sip-i' | 'sip-t';
  targetProtocol: 'tdm' | 'pri' | 'isup' | 'sip' | 'sip-i' | 'sip-t';
  callSession: CallSession;
  variant?: string;
}

export class ProtocolTranslator extends EventEmitter {
  private logger: Logger;
  private translationRules: Map<string, ProtocolTranslationRules> = new Map();

  constructor(logger: Logger) {
    super();
    this.logger = logger;
    this.initializeTranslationRules();
  }

  private initializeTranslationRules(): void {
    // ITU-T Q.850 to SIP cause code mappings
    this.setupITUToSIPCauseCodes();
    
    // ANSI to SIP cause code mappings
    this.setupANSIToSIPCauseCodes();
    
    // ETSI specific mappings
    this.setupETSIToSIPCauseCodes();
    
    // Progress indicator mappings
    this.setupProgressIndicatorMappings();
    
    // Numbering plan mappings
    this.setupNumberingPlanMappings();
  }

  private setupITUToSIPCauseCodes(): void {
    const rules: ProtocolTranslationRules = {
      causeCodeMapping: new Map([
        // Normal call clearing scenarios
        [16, 200],  // Normal call clearing -> 200 OK
        [17, 486],  // User busy -> 486 Busy Here
        [18, 408],  // No user responding -> 408 Request Timeout
        [19, 480],  // No answer from user -> 480 Temporarily Unavailable
        [20, 480],  // Subscriber absent -> 480 Temporarily Unavailable
        [21, 603],  // Call rejected -> 603 Decline
        [22, 410],  // Number changed -> 410 Gone
        [23, 410],  // Redirection to new destination -> 301 Moved Permanently
        
        // Network/service issues
        [1, 404],   // Unallocated number -> 404 Not Found
        [2, 404],   // No route to specified transit network -> 404 Not Found
        [3, 404],   // No route to destination -> 404 Not Found
        [27, 502],  // Destination out of order -> 502 Bad Gateway
        [28, 484],  // Invalid number format -> 484 Address Incomplete
        [29, 501],  // Facility rejected -> 501 Not Implemented
        [31, 500],  // Normal, unspecified -> 500 Server Internal Error
        
        // Resource unavailable
        [34, 503],  // No circuit/channel available -> 503 Service Unavailable
        [38, 503],  // Network out of order -> 503 Service Unavailable
        [41, 503],  // Temporary failure -> 503 Service Unavailable
        [42, 503],  // Switching equipment congestion -> 503 Service Unavailable
        [43, 503],  // Access information discarded -> 503 Service Unavailable
        [44, 503],  // Requested circuit/channel not available -> 503 Service Unavailable
        [47, 503],  // Resources unavailable -> 503 Service Unavailable
        
        // Service/option not available
        [50, 501],  // Requested facility not subscribed -> 501 Not Implemented
        [57, 403],  // Bearer capability not authorized -> 403 Forbidden
        [58, 488],  // Bearer capability not presently available -> 488 Not Acceptable Here
        [63, 500],  // Service/option not available -> 500 Server Internal Error
        
        // Service/option not implemented
        [65, 501],  // Bearer capability not implemented -> 501 Not Implemented
        [66, 415],  // Channel type not implemented -> 415 Unsupported Media Type
        [69, 501],  // Requested facility not implemented -> 501 Not Implemented
        [70, 501],  // Only restricted digital info bearer available -> 501 Not Implemented
        [79, 501],  // Service/option not implemented -> 501 Not Implemented
        
        // Invalid message
        [81, 400],  // Invalid call reference value -> 400 Bad Request
        [82, 481],  // Identified channel does not exist -> 481 Call/Transaction Does Not Exist
        [83, 481],  // Suspended call exists but this call identity does not -> 481 Call/Transaction Does Not Exist
        [84, 481],  // Call identity in use -> 481 Call/Transaction Does Not Exist
        [85, 481],  // No call suspended -> 481 Call/Transaction Does Not Exist
        [86, 481],  // Call having requested call identity has been cleared -> 481 Call/Transaction Does Not Exist
        [87, 403],  // User not member of CUG -> 403 Forbidden
        [88, 488],  // Incompatible destination -> 488 Not Acceptable Here
        [90, 400],  // Non-existent CUG -> 400 Bad Request
        [91, 502],  // Invalid transit network selection -> 502 Bad Gateway
        [95, 400],  // Invalid message, unspecified -> 400 Bad Request
        
        // Protocol error
        [96, 400],  // Mandatory information element is missing -> 400 Bad Request
        [97, 501],  // Message type non-existent or not implemented -> 501 Not Implemented
        [98, 400],  // Message not compatible with call state -> 400 Bad Request
        [99, 400],  // Information element non-existent -> 400 Bad Request
        [100, 400], // Invalid information element contents -> 400 Bad Request
        [101, 400], // Message not compatible with call state -> 400 Bad Request
        [102, 504], // Recovery on timer expiry -> 504 Server Timeout
        [103, 500], // Parameter non-existent or not implemented -> 500 Server Internal Error
        [110, 400], // Message with unrecognized parameter -> 400 Bad Request
        [111, 500], // Protocol error, unspecified -> 500 Server Internal Error
        
        // Interworking
        [127, 500] // Interworking, unspecified -> 500 Server Internal Error
      ]),
      progressIndicatorMapping: new Map([
        [1, 100],   // Call is not end-to-end ISDN -> 100 Trying
        [2, 180],   // Destination address is non-ISDN -> 180 Ringing
        [3, 183],   // Origination address is non-ISDN -> 183 Session Progress
        [4, 183],   // Call has returned to the ISDN -> 183 Session Progress
        [8, 183]    // In-band information or appropriate pattern available -> 183 Session Progress
      ]),
      numberingPlanMapping: new Map([
        ['unknown', 0],
        ['isdn', 1],
        ['data', 3],
        ['telex', 4],
        ['national', 8],
        ['private', 9]
      ]),
      natureOfAddressMapping: new Map([
        ['unknown', 0],
        ['international', 4],
        ['national', 3],
        ['subscriber', 1],
        ['abbreviated', 6]
      ])
    };
    
    this.translationRules.set('itu-to-sip', rules);
  }

  private setupANSIToSIPCauseCodes(): void {
    const rules: ProtocolTranslationRules = {
      causeCodeMapping: new Map([
        // ANSI specific cause codes
        [0, 200],   // Valid -> 200 OK
        [1, 404],   // Unallocated destination number -> 404 Not Found
        [2, 502],   // Unknown destination message type -> 502 Bad Gateway
        [3, 502],   // Unknown destination signaling point code -> 502 Bad Gateway
        [4, 503],   // Unknown destination subsystem number -> 503 Service Unavailable
        [5, 503],   // Unknown destination -> 503 Service Unavailable
        [6, 503],   // Subsystem congestion -> 503 Service Unavailable
        [7, 503],   // Subsystem failure -> 503 Service Unavailable
        [8, 403],   // Unequipped failure -> 403 Forbidden
        [9, 500],   // Unknown error -> 500 Server Internal Error
        [10, 503],  // Hop counter violation -> 503 Service Unavailable
        [11, 400],  // No translation for an address of such nature -> 400 Bad Request
        [12, 400],  // No translation for this specific address -> 400 Bad Request
        [13, 503],  // Subsystem congestion -> 503 Service Unavailable
        [14, 503],  // Subsystem failure -> 503 Service Unavailable
        [15, 403],  // Unequipped user -> 403 Forbidden
        [16, 404],  // Destination does not support the requested procedure -> 404 Not Found
        [17, 400],  // Unknown procedure -> 400 Bad Request
        [18, 504],  // Timer expiry -> 504 Server Timeout
        [19, 500]   // Improper caller response -> 500 Server Internal Error
      ]),
      progressIndicatorMapping: new Map([
        [0, 100],   // No progress -> 100 Trying
        [1, 183],   // Call establishment in progress -> 183 Session Progress
        [2, 180],   // Alerting -> 180 Ringing
        [3, 200],   // Connection -> 200 OK
        [4, 183]    // Call establishment complete -> 183 Session Progress
      ]),
      numberingPlanMapping: new Map([
        ['unknown', 0],
        ['isdn', 1],
        ['telephony', 1],
        ['data', 3],
        ['telex', 4],
        ['maritime_mobile', 5],
        ['land_mobile', 6],
        ['private', 9]
      ]),
      natureOfAddressMapping: new Map([
        ['unknown', 0],
        ['subscriber', 1],
        ['national', 3],
        ['international', 4]
      ])
    };
    
    this.translationRules.set('ansi-to-sip', rules);
  }

  private setupETSIToSIPCauseCodes(): void {
    const rules: ProtocolTranslationRules = {
      causeCodeMapping: new Map([
        // ETSI EN 300 403-1 specific mappings
        [1, 404],   // Unallocated/unassigned number -> 404 Not Found
        [16, 200],  // Normal call clearing -> 200 OK
        [17, 486],  // User busy -> 486 Busy Here
        [18, 408],  // No user responding -> 408 Request Timeout
        [19, 480],  // No answer from user -> 480 Temporarily Unavailable
        [21, 603],  // Call rejected -> 603 Decline
        [22, 301],  // Number changed -> 301 Moved Permanently
        [26, 486],  // Non-selected user clearing -> 486 Busy Here
        [27, 502],  // Destination out of order -> 502 Bad Gateway
        [28, 484],  // Invalid number format -> 484 Address Incomplete
        [29, 501],  // Facility rejected -> 501 Not Implemented
        [30, 183],  // Response to STATUS ENQUIRY -> 183 Session Progress
        [31, 500],  // Normal, unspecified -> 500 Server Internal Error
        [34, 503],  // No circuit/channel available -> 503 Service Unavailable
        [38, 503],  // Network out of order -> 503 Service Unavailable
        [39, 503],  // Permanent frame mode connection out of service -> 503 Service Unavailable
        [40, 503],  // Permanent frame mode connection operational -> 503 Service Unavailable
        [41, 503],  // Temporary failure -> 503 Service Unavailable
        [42, 503],  // Switching equipment congestion -> 503 Service Unavailable
        [43, 503],  // Access information discarded -> 503 Service Unavailable
        [44, 503],  // Requested circuit/channel not available -> 503 Service Unavailable
        [46, 503],  // Precedence call blocked -> 503 Service Unavailable
        [47, 503]   // Resource unavailable, unspecified -> 503 Service Unavailable
      ]),
      progressIndicatorMapping: new Map([
        [1, 100],   // Call is not end-to-end ISDN -> 100 Trying
        [2, 180],   // Destination address is non-ISDN -> 180 Ringing
        [3, 183],   // Origination address is non-ISDN -> 183 Session Progress
        [4, 183],   // Call has returned to the ISDN -> 183 Session Progress
        [5, 183],   // Interworking occurred and has been encountered -> 183 Session Progress
        [8, 183]    // In-band information or appropriate pattern available -> 183 Session Progress
      ]),
      numberingPlanMapping: new Map([
        ['unknown', 0],
        ['isdn', 1],
        ['data', 3],
        ['telex', 4],
        ['national', 8],
        ['private', 9],
        ['reserved', 15]
      ]),
      natureOfAddressMapping: new Map([
        ['unknown', 0],
        ['subscriber', 1],
        ['unknown_reserved', 2],
        ['national', 3],
        ['international', 4],
        ['network_specific', 5],
        ['abbreviated', 6]
      ])
    };
    
    this.translationRules.set('etsi-to-sip', rules);
  }

  private setupProgressIndicatorMappings(): void {
    // Additional progress indicator mappings for different variants
    const progressMappings = new Map([
      // Common progress indicators
      [0, 100],   // No progress -> 100 Trying
      [1, 100],   // Call is not end-to-end ISDN -> 100 Trying
      [2, 180],   // Destination address is non-ISDN -> 180 Ringing
      [3, 183],   // Origination address is non-ISDN -> 183 Session Progress
      [4, 183],   // Call has returned to the ISDN -> 183 Session Progress
      [5, 183],   // Interworking occurred -> 183 Session Progress
      [6, 183],   // Interworking encountered at this signaling point -> 183 Session Progress
      [7, 183],   // Interworking encountered at previous signaling point -> 183 Session Progress
      [8, 183]    // In-band information available -> 183 Session Progress
    ]);
    
    // Apply to all variants
    for (const ruleKey of this.translationRules.keys()) {
      const rules = this.translationRules.get(ruleKey);
      if (rules) {
        rules.progressIndicatorMapping = new Map([...rules.progressIndicatorMapping, ...progressMappings]);
      }
    }
  }

  private setupNumberingPlanMappings(): void {
    // Standard numbering plan mappings (ITU-T E.164)
    const numberingPlans = new Map([
      ['unknown', 0],
      ['isdn', 1],
      ['spare_2', 2],
      ['data', 3],
      ['telex', 4],
      ['maritime_mobile', 5],
      ['land_mobile', 6],
      ['spare_7', 7],
      ['national', 8],
      ['private', 9],
      ['ermes', 10],
      ['spare_11', 11],
      ['spare_12', 12],
      ['spare_13', 13],
      ['spare_14', 14],
      ['reserved', 15]
    ]);
    
    // Apply to all variants
    for (const ruleKey of this.translationRules.keys()) {
      const rules = this.translationRules.get(ruleKey);
      if (rules) {
        rules.numberingPlanMapping = numberingPlans;
      }
    }
  }

  // Main translation methods
  translatePRIToSIP(priMessage: PRIMessage, context: TranslationContext): SIPMessage {
    const variant = context.variant || 'itu';
    const rules = this.translationRules.get(`${variant}-to-sip`);
    
    if (!rules) {
      throw new Error(`No translation rules found for variant: ${variant}`);
    }

    let sipMessage: SIPMessage;

    switch (priMessage.messageType) {
      case 'setup':
        sipMessage = this.translateSetupToInvite(priMessage, rules, context);
        break;
      case 'call_proceeding':
        sipMessage = this.translateCallProceedingToTrying(priMessage, rules, context);
        break;
      case 'alerting':
        sipMessage = this.translateAlertingToRinging(priMessage, rules, context);
        break;
      case 'connect':
        sipMessage = this.translateConnectToOK(priMessage, rules, context);
        break;
      case 'disconnect':
      case 'release':
        sipMessage = this.translateDisconnectToBye(priMessage, rules, context);
        break;
      default:
        throw new Error(`Unsupported PRI message type: ${priMessage.messageType}`);
    }

    this.emit('translated', 'pri-to-sip', priMessage, sipMessage, context);
    return sipMessage;
  }

  translateSIPToPRI(sipMessage: SIPMessage, context: TranslationContext): PRIMessage {
    const variant = context.variant || 'itu';
    
    let priMessage: PRIMessage;

    if (sipMessage.method === 'INVITE') {
      priMessage = this.translateInviteToSetup(sipMessage, context);
    } else if (sipMessage.method.match(/^1\d{2}$/)) {
      priMessage = this.translateProgressToCallProceeding(sipMessage, context);
    } else if (sipMessage.method.match(/^180$/)) {
      priMessage = this.translateRingingToAlerting(sipMessage, context);
    } else if (sipMessage.method.match(/^200$/)) {
      priMessage = this.translateOKToConnect(sipMessage, context);
    } else if (sipMessage.method === 'BYE' || sipMessage.method.match(/^[4-6]\d{2}$/)) {
      priMessage = this.translateByeToDisconnect(sipMessage, context);
    } else {
      throw new Error(`Unsupported SIP message: ${sipMessage.method}`);
    }

    this.emit('translated', 'sip-to-pri', sipMessage, priMessage, context);
    return priMessage;
  }

  translateISUPToSIPT(isupMessage: ISUPMessage, context: TranslationContext): SIPMessage {
    const variant = context.variant || 'itu';
    const rules = this.translationRules.get(`${variant}-to-sip`);
    
    if (!rules) {
      throw new Error(`No translation rules found for variant: ${variant}`);
    }

    let sipMessage: SIPMessage;

    switch (isupMessage.messageType) {
      case 0x01: // IAM
        sipMessage = this.translateIAMToSIPT(isupMessage, rules, context);
        break;
      case 0x06: // ACM
        sipMessage = this.translateACMToProgress(isupMessage, rules, context);
        break;
      case 0x09: // ANM
        sipMessage = this.translateANMToOK(isupMessage, rules, context);
        break;
      case 0x0C: // REL
        sipMessage = this.translateRELToBye(isupMessage, rules, context);
        break;
      default:
        throw new Error(`Unsupported ISUP message type: ${isupMessage.messageType}`);
    }

    this.emit('translated', 'isup-to-sip-t', isupMessage, sipMessage, context);
    return sipMessage;
  }

  // Helper methods for specific translations
  private translateSetupToInvite(priMessage: PRIMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    const callId = `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    
    return {
      method: 'INVITE',
      uri: `sip:${priMessage.calledNumber}@${context.callSession.sipCallId?.split('@')[1] || 'localhost'}`,
      headers: {
        'Call-ID': callId,
        'From': `<sip:${priMessage.callingNumber}@localhost>;tag=${Math.random().toString(36).substr(2, 9)}`,
        'To': `<sip:${priMessage.calledNumber}@localhost>`,
        'Via': 'SIP/2.0/UDP localhost:5060',
        'CSeq': '1 INVITE',
        'Content-Type': 'application/sdp',
        'P-Asserted-Identity': `<sip:${priMessage.callingNumber}@localhost>`
      },
      body: this.generateSDPOffer(context)
    };
  }

  private translateCallProceedingToTrying(priMessage: PRIMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    return {
      method: '100 Trying',
      uri: '',
      headers: {
        'Call-ID': context.callSession.sipCallId || '',
        'CSeq': '1 INVITE'
      }
    };
  }

  private translateAlertingToRinging(priMessage: PRIMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    return {
      method: '180 Ringing',
      uri: '',
      headers: {
        'Call-ID': context.callSession.sipCallId || '',
        'CSeq': '1 INVITE'
      }
    };
  }

  private translateConnectToOK(priMessage: PRIMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    return {
      method: '200 OK',
      uri: '',
      headers: {
        'Call-ID': context.callSession.sipCallId || '',
        'CSeq': '1 INVITE',
        'Content-Type': 'application/sdp'
      },
      body: this.generateSDPAnswer(context)
    };
  }

  private translateDisconnectToBye(priMessage: PRIMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    const cause = priMessage.cause || 16;
    const sipCode = rules.causeCodeMapping.get(cause) || 500;
    
    if (sipCode === 200) {
      return {
        method: 'BYE',
        uri: '',
        headers: {
          'Call-ID': context.callSession.sipCallId || '',
          'CSeq': '2 BYE'
        }
      };
    } else {
      return {
        method: `${sipCode} ${this.getSIPReasonPhrase(sipCode)}`,
        uri: '',
        headers: {
          'Call-ID': context.callSession.sipCallId || '',
          'CSeq': '1 INVITE'
        }
      };
    }
  }

  private translateIAMToSIPT(isupMessage: ISUPMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    const callId = `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    
    // Parse ISUP parameters
    const { callingNumber, calledNumber } = this.parseIAMParameters(isupMessage.parameters);
    
    return {
      method: 'INVITE',
      uri: `sip:${calledNumber}@localhost`,
      headers: {
        'Call-ID': callId,
        'From': `<sip:${callingNumber}@localhost>;tag=${Math.random().toString(36).substr(2, 9)}`,
        'To': `<sip:${calledNumber}@localhost>`,
        'Via': 'SIP/2.0/UDP localhost:5060',
        'CSeq': '1 INVITE',
        'Content-Type': 'application/sdp',
        'Content-Disposition': 'session;handling=required',
        'P-Asserted-Identity': `<sip:${callingNumber}@localhost>`,
        // SIP-T specific headers
        'Content-Encoding': 'binary',
        'MIME-Version': '1.0'
      },
      body: this.generateSIPTBody(isupMessage, context)
    };
  }

  private translateACMToProgress(isupMessage: ISUPMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    return {
      method: '183 Session Progress',
      uri: '',
      headers: {
        'Call-ID': context.callSession.sipCallId || '',
        'CSeq': '1 INVITE'
      }
    };
  }

  private translateANMToOK(isupMessage: ISUPMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    return {
      method: '200 OK',
      uri: '',
      headers: {
        'Call-ID': context.callSession.sipCallId || '',
        'CSeq': '1 INVITE',
        'Content-Type': 'application/sdp'
      },
      body: this.generateSDPAnswer(context)
    };
  }

  private translateRELToBye(isupMessage: ISUPMessage, rules: ProtocolTranslationRules, context: TranslationContext): SIPMessage {
    const cause = this.parseISUPCause(isupMessage.parameters);
    const sipCode = rules.causeCodeMapping.get(cause) || 500;
    
    return {
      method: 'BYE',
      uri: '',
      headers: {
        'Call-ID': context.callSession.sipCallId || '',
        'CSeq': '2 BYE',
        'Reason': `Q.850;cause=${cause};text="${this.getQ850ReasonText(cause)}"`
      }
    };
  }

  // Reverse translations (SIP to TDM protocols)
  private translateInviteToSetup(sipMessage: SIPMessage, context: TranslationContext): PRIMessage {
    const fromHeader = sipMessage.headers['From'];
    const toHeader = sipMessage.headers['To'];
    
    const callingNumber = this.extractNumberFromSIPHeader(fromHeader);
    const calledNumber = this.extractNumberFromSIPHeader(toHeader);
    
    return {
      messageType: 'setup',
      callReference: context.callSession.id ? parseInt(context.callSession.id) : Math.floor(Math.random() * 32767),
      callingNumber,
      calledNumber,
      bearerCapability: 'speech'
    };
  }

  private translateProgressToCallProceeding(sipMessage: SIPMessage, context: TranslationContext): PRIMessage {
    return {
      messageType: 'call_proceeding',
      callReference: context.callSession.id ? parseInt(context.callSession.id) : Math.floor(Math.random() * 32767)
    };
  }

  private translateRingingToAlerting(sipMessage: SIPMessage, context: TranslationContext): PRIMessage {
    return {
      messageType: 'alerting',
      callReference: context.callSession.id ? parseInt(context.callSession.id) : Math.floor(Math.random() * 32767)
    };
  }

  private translateOKToConnect(sipMessage: SIPMessage, context: TranslationContext): PRIMessage {
    return {
      messageType: 'connect',
      callReference: context.callSession.id ? parseInt(context.callSession.id) : Math.floor(Math.random() * 32767)
    };
  }

  private translateByeToDisconnect(sipMessage: SIPMessage, context: TranslationContext): PRIMessage {
    const cause = this.extractSIPCauseCode(sipMessage);
    
    return {
      messageType: 'disconnect',
      callReference: context.callSession.id ? parseInt(context.callSession.id) : Math.floor(Math.random() * 32767),
      cause
    };
  }

  // Utility methods
  private parseIAMParameters(parameters: Buffer): { callingNumber?: string; calledNumber?: string } {
    // Simplified ISUP parameter parsing
    return {
      callingNumber: '1234567890',
      calledNumber: '0987654321'
    };
  }

  private parseISUPCause(parameters: Buffer): number {
    // Parse ISUP cause parameter
    return 16; // Default to normal call clearing
  }

  private extractNumberFromSIPHeader(header: string): string {
    const match = header.match(/sip:([^@;]+)/);
    return match ? match[1] : '';
  }

  private extractSIPCauseCode(sipMessage: SIPMessage): number {
    if (sipMessage.method === 'BYE') {
      return 16; // Normal call clearing
    }
    
    const statusCode = parseInt(sipMessage.method.split(' ')[0]);
    
    // Map common SIP status codes to Q.850 cause codes
    const sipToCauseMap: { [key: number]: number } = {
      200: 16, // OK -> Normal call clearing
      400: 95, // Bad Request -> Invalid message
      401: 57, // Unauthorized -> Bearer capability not authorized
      403: 57, // Forbidden -> Bearer capability not authorized
      404: 1,  // Not Found -> Unallocated number
      408: 18, // Request Timeout -> No user responding
      410: 22, // Gone -> Number changed
      415: 66, // Unsupported Media Type -> Channel type not implemented
      480: 19, // Temporarily Unavailable -> No answer from user
      481: 82, // Call/Transaction Does Not Exist -> Identified channel does not exist
      486: 17, // Busy Here -> User busy
      488: 88, // Not Acceptable Here -> Incompatible destination
      500: 31, // Server Internal Error -> Normal, unspecified
      501: 79, // Not Implemented -> Service/option not implemented
      502: 38, // Bad Gateway -> Network out of order
      503: 34, // Service Unavailable -> No circuit/channel available
      504: 102 // Server Timeout -> Recovery on timer expiry
    };
    
    return sipToCauseMap[statusCode] || 31;
  }

  private generateSDPOffer(context: TranslationContext): string {
    return `v=0
o=redfire ${Date.now()} ${Date.now()} IN IP4 127.0.0.1
s=Call
c=IN IP4 127.0.0.1
t=0 0
m=audio ${context.callSession.rtpSession?.localPort || 10000} RTP/AVP 0 8
a=rtpmap:0 PCMU/8000
a=rtpmap:8 PCMA/8000
a=sendrecv`;
  }

  private generateSDPAnswer(context: TranslationContext): string {
    return this.generateSDPOffer(context);
  }

  private generateSIPTBody(isupMessage: ISUPMessage, context: TranslationContext): string {
    // Generate multipart MIME body for SIP-T
    const boundary = `----=_Part_${Date.now()}_${Math.random().toString(36)}`;
    
    return `--${boundary}
Content-Type: application/sdp

${this.generateSDPOffer(context)}

--${boundary}
Content-Type: application/ISUP
Content-Disposition: signal;handling=required

${isupMessage.parameters.toString('base64')}
--${boundary}--`;
  }

  private getSIPReasonPhrase(statusCode: number): string {
    const reasonPhrases: { [key: number]: string } = {
      200: 'OK',
      400: 'Bad Request',
      403: 'Forbidden',
      404: 'Not Found',
      408: 'Request Timeout',
      480: 'Temporarily Unavailable',
      486: 'Busy Here',
      500: 'Server Internal Error',
      501: 'Not Implemented',
      502: 'Bad Gateway',
      503: 'Service Unavailable',
      504: 'Server Timeout'
    };
    
    return reasonPhrases[statusCode] || 'Unknown';
  }

  private getQ850ReasonText(cause: number): string {
    const reasonTexts: { [key: number]: string } = {
      1: 'Unallocated number',
      16: 'Normal call clearing',
      17: 'User busy',
      18: 'No user responding',
      19: 'No answer from user',
      21: 'Call rejected',
      22: 'Number changed',
      27: 'Destination out of order',
      28: 'Invalid number format',
      31: 'Normal, unspecified',
      34: 'No circuit/channel available',
      38: 'Network out of order',
      41: 'Temporary failure',
      42: 'Switching equipment congestion'
    };
    
    return reasonTexts[cause] || 'Unknown';
  }

  // Public API methods
  getTranslationRules(variant: string): ProtocolTranslationRules | undefined {
    return this.translationRules.get(`${variant}-to-sip`);
  }

  addCustomCauseMapping(variant: string, tdmCause: number, sipCode: number): void {
    const rules = this.translationRules.get(`${variant}-to-sip`);
    if (rules) {
      rules.causeCodeMapping.set(tdmCause, sipCode);
    }
  }

  updateTranslationRules(variant: string, rules: Partial<ProtocolTranslationRules>): void {
    const existingRules = this.translationRules.get(`${variant}-to-sip`);
    if (existingRules) {
      if (rules.causeCodeMapping) {
        existingRules.causeCodeMapping = new Map([...existingRules.causeCodeMapping, ...rules.causeCodeMapping]);
      }
      if (rules.progressIndicatorMapping) {
        existingRules.progressIndicatorMapping = new Map([...existingRules.progressIndicatorMapping, ...rules.progressIndicatorMapping]);
      }
      if (rules.numberingPlanMapping) {
        existingRules.numberingPlanMapping = new Map([...existingRules.numberingPlanMapping, ...rules.numberingPlanMapping]);
      }
      if (rules.natureOfAddressMapping) {
        existingRules.natureOfAddressMapping = new Map([...existingRules.natureOfAddressMapping, ...rules.natureOfAddressMapping]);
      }
    }
  }
}