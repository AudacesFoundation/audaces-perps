import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  PublicKey,
  SYSVAR_CLOCK_PUBKEY,
  TransactionInstruction,
} from "@solana/web3.js";
import BN from "bn.js";
import { deserializeUnchecked, Schema, serialize } from "borsh";
import { Numberu64 } from "./utils";

export enum PositionType {
  Short = 0,
  Long = 1,
}

export const BONFIDA_BNB = new PublicKey(
  "FxqKVkCMtTVmJ6cEibvQeNJCtT4JWEzJzhZ3bFNmR6zu"
);

export const LIQUIDATION_LABEL = new PublicKey(
  "LiquidationRecord11111111111111111111111111"
);

export const FUNDING_LABEL = new PublicKey(
  "FundingRecord1111111111111111111111111111111"
);

export const TRADE_LABEL = new PublicKey(
  "TradeRecord11111111111111111111111111111111"
);

export const FUNDING_EXTRACTION_LABEL = new PublicKey(
  "FundingExtraction111111111111111111111111111"
);

export class createMarketInstruction {
  tag: number;
  signerNonce: number;
  marketSymbol: string;
  initialVPcAmount: Numberu64;
  coinDecimals: number;
  quoteDecimals: number;
  static schema: Schema = new Map([
    [
      createMarketInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["signerNonce", "u8"],
          ["marketSymbol", "string"],
          ["initialVPcAmount", "u64"],
          ["coinDecimals", "u8"],
          ["quoteDecimals", "u8"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    signerNonce: number;
    marketSymbol: string;
    initialVPcAmount: Numberu64;
    coinDecimals: number;
    quoteDecimals: number;
  }) {
    this.tag = 0;
    this.signerNonce = obj.signerNonce;
    this.marketSymbol = obj.marketSymbol;
    this.initialVPcAmount = obj.initialVPcAmount;
    this.coinDecimals = obj.coinDecimals;
    this.quoteDecimals = obj.quoteDecimals;
  }

  serialize(): Uint8Array {
    return serialize(createMarketInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    marketAccount: PublicKey,
    oracleAccount: PublicKey,
    adminAccount: PublicKey,
    marketVault: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: SYSVAR_CLOCK_PUBKEY,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: adminAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class addInstanceInstruction {
  tag: number;
  static schema: Schema = new Map([
    [
      addInstanceInstruction,
      {
        kind: "struct",
        fields: [["tag", "u8"]],
      },
    ],
  ]);

  constructor() {
    this.tag = 1;
  }

  serialize(): Uint8Array {
    return serialize(addInstanceInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    marketAccount: PublicKey,
    instanceAccount: PublicKey,
    marketAdmin: PublicKey,
    memory_pages: PublicKey[]
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketAdmin,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
    ];
    keys = keys.concat(
      memory_pages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class updateOracleAccountInstruction {
  tag: number;
  static schema: Schema = new Map([
    [
      updateOracleAccountInstruction,
      {
        kind: "struct",
        fields: [["tag", "u8"]],
      },
    ],
  ]);

  constructor() {
    this.tag = 2;
  }

  serialize(): Uint8Array {
    return serialize(updateOracleAccountInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    marketAccount: PublicKey,
    pythOracleMappingAccount: PublicKey,
    pythOracleProductAccount: PublicKey,
    pythOraclePriceAccount: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: pythOracleMappingAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: pythOracleProductAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: pythOraclePriceAccount,
        isSigner: false,
        isWritable: false,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class openPositionInstruction {
  tag: number;
  side: PositionType;
  collateral: Numberu64;
  instanceIndex: number;
  leverage: Numberu64;
  predictedEntryPrice: Numberu64;
  maximumSlippageMargin: Numberu64;
  static schema: Schema = new Map([
    [
      openPositionInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["side", "u8"],
          ["collateral", "u64"],
          ["instanceIndex", "u8"],
          ["leverage", "u64"],
          ["predictedEntryPrice", "u64"],
          ["maximumSlippageMargin", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    side: PositionType;
    collateral: Numberu64;
    instanceIndex: number;
    leverage: Numberu64;
    predictedEntryPrice: Numberu64;
    maximumSlippageMargin: Numberu64;
  }) {
    this.tag = 3;
    this.side = obj.side;
    this.collateral = obj.collateral;
    this.instanceIndex = obj.instanceIndex;
    this.leverage = obj.leverage;
    this.predictedEntryPrice = obj.predictedEntryPrice;
    this.maximumSlippageMargin = obj.maximumSlippageMargin;
  }

  serialize(): Uint8Array {
    return serialize(openPositionInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    marketAccount: PublicKey,
    instanceAccount: PublicKey,
    marketSigner: PublicKey,
    marketVault: PublicKey,
    userAccountOwner: PublicKey,
    userAccount: PublicKey,
    memoryPages: PublicKey[],
    bonfida_bnb: PublicKey,
    oracleAccount: PublicKey,
    discountAccount?: PublicKey,
    discountAccountOwner?: PublicKey,
    referrerAccount?: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: TOKEN_PROGRAM_ID,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: SYSVAR_CLOCK_PUBKEY,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketSigner,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: bonfida_bnb,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: userAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: TRADE_LABEL,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
    ];
    keys = keys.concat(
      memoryPages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    if (!!discountAccount) {
      if (!discountAccountOwner) {
        throw "The owner of the discount account must be specified as well";
      }
      keys.push({
        pubkey: discountAccount,
        isSigner: false,
        isWritable: false,
      });
      keys.push({
        pubkey: discountAccountOwner,
        isSigner: true,
        isWritable: false,
      });
    }

    if (!!referrerAccount) {
      keys.push({
        pubkey: referrerAccount,
        isSigner: false,
        isWritable: true,
      });
    }

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class addBudgetInstruction {
  tag: number;
  amount: BN;
  static schema: Schema = new Map([
    [
      addBudgetInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["amount", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: { amount: Numberu64 }) {
    this.amount = obj.amount;
    this.tag = 4;
  }

  serialize(): Uint8Array {
    return serialize(addBudgetInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    spl_token_program: PublicKey,
    marketAccount: PublicKey,
    marketVault: PublicKey,
    sourceTokenAccount: PublicKey,
    sourceOwner: PublicKey,
    userAccount: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: spl_token_program,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: sourceOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: sourceTokenAccount,
        isSigner: false,
        isWritable: true,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class withdrawBudgetInstruction {
  tag: number;
  amount: BN;
  static schema: Schema = new Map([
    [
      withdrawBudgetInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["amount", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: { amount: Numberu64 }) {
    this.amount = obj.amount;
    this.tag = 5;
  }

  serialize(): Uint8Array {
    return serialize(withdrawBudgetInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    spl_token_program: PublicKey,
    marketAccount: PublicKey,
    marketVault: PublicKey,
    targetTokenAccount: PublicKey,
    marketSigner: PublicKey,
    userAccount: PublicKey,
    userAccountOwner: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: spl_token_program,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketSigner,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: userAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: targetTokenAccount,
        isSigner: false,
        isWritable: true,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class increasePositionInstruction {
  tag: number;
  positionIndex: Uint8Array;
  addCollateral: Numberu64;
  instanceIndex: number;
  leverage: Numberu64;
  predictedEntryPrice: Numberu64;
  maximumSlippageMargin: Numberu64;
  static schema: Schema = new Map([
    [
      increasePositionInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["addCollateral", "u64"],
          ["instanceIndex", "u8"],
          ["leverage", "u64"],
          ["positionIndex", [2]],
          ["predictedEntryPrice", "u64"],
          ["maximumSlippageMargin", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    addCollateral: Numberu64;
    instanceIndex: number;
    leverage: Numberu64;
    positionIndex: Uint8Array;
    predictedEntryPrice: Numberu64;
    maximumSlippageMargin: Numberu64;
  }) {
    this.tag = 6;
    this.addCollateral = obj.addCollateral;
    this.instanceIndex = obj.instanceIndex;
    this.leverage = obj.leverage;
    this.positionIndex = obj.positionIndex;
    this.predictedEntryPrice = obj.predictedEntryPrice;
    this.maximumSlippageMargin = obj.maximumSlippageMargin;
  }

  serialize(): Uint8Array {
    return serialize(increasePositionInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    clock_sysvar: PublicKey,
    marketAccount: PublicKey,
    marketSigner: PublicKey,
    marketVault: PublicKey,
    bonfida_bnb: PublicKey,
    instanceAccount: PublicKey,
    userAccount: PublicKey,
    userAccountOwner: PublicKey,
    oracleAccount: PublicKey,
    memoryPages: PublicKey[],
    discountAccount?: PublicKey,
    discountAccountOwner?: PublicKey,
    referrerAccount?: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: TOKEN_PROGRAM_ID,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: clock_sysvar,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketSigner,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: bonfida_bnb,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: userAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: TRADE_LABEL,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
    ];
    keys = keys.concat(
      memoryPages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    if (!!discountAccount) {
      if (!discountAccountOwner) {
        throw "The owner of the discount account must be specified as well";
      }
      keys.push({
        pubkey: discountAccount,
        isSigner: false,
        isWritable: false,
      });
      keys.push({
        pubkey: discountAccountOwner,
        isSigner: true,
        isWritable: false,
      });
    }

    if (!!referrerAccount) {
      keys.push({
        pubkey: referrerAccount,
        isSigner: false,
        isWritable: true,
      });
    }

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class closePositionInstruction {
  tag: number;
  positionIndex: Uint8Array;
  closingCollateral: Numberu64;
  closingVCoin: Numberu64;
  predictedEntryPrice: Numberu64;
  maximumSlippageMargin: Numberu64;
  static schema: Schema = new Map([
    [
      closePositionInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["positionIndex", [2]],
          ["closingCollateral", "u64"],
          ["closingVCoin", "u64"],
          ["predictedEntryPrice", "u64"],
          ["maximumSlippageMargin", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    positionIndex: Uint8Array;
    closingCollateral: Numberu64;
    closingVCoin: Numberu64;
    predictedEntryPrice: Numberu64;
    maximumSlippageMargin: Numberu64;
  }) {
    this.tag = 7;
    this.positionIndex = obj.positionIndex;
    this.closingCollateral = obj.closingCollateral;
    this.closingVCoin = obj.closingVCoin;
    this.predictedEntryPrice = obj.predictedEntryPrice;
    this.maximumSlippageMargin = obj.maximumSlippageMargin;
  }

  serialize(): Uint8Array {
    return serialize(closePositionInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    marketSigner: PublicKey,
    marketVault: PublicKey,
    oracleAccount: PublicKey,
    marketAccount: PublicKey,
    instanceAccount: PublicKey,
    positionOwner: PublicKey,
    userAccount: PublicKey,
    memory_pages: PublicKey[],
    bonfida_bnb: PublicKey,
    discountAccount?: PublicKey,
    discountAccountOwner?: PublicKey,
    referrerAccount?: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: TOKEN_PROGRAM_ID,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: SYSVAR_CLOCK_PUBKEY,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketSigner,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: bonfida_bnb,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: positionOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: TRADE_LABEL,
        isSigner: false,
        isWritable: false,
      },
    ];
    keys = keys.concat(
      memory_pages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    if (!!discountAccount) {
      if (!discountAccountOwner) {
        throw "The owner of the discount account must be specified as well";
      }
      keys.push({
        pubkey: discountAccount,
        isSigner: false,
        isWritable: false,
      });
      keys.push({
        pubkey: discountAccountOwner,
        isSigner: true,
        isWritable: false,
      });
    }

    if (!!referrerAccount) {
      keys.push({
        pubkey: referrerAccount,
        isSigner: false,
        isWritable: true,
      });
    }

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class collectGarbageInstruction {
  tag: number;
  instanceIndex: number;
  maxIterations: BN;
  static schema: Schema = new Map([
    [
      collectGarbageInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["instanceIndex", "u8"],
          ["maxIterations", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: { instanceIndex: number; maxIterations: Numberu64 }) {
    this.maxIterations = obj.maxIterations;
    this.instanceIndex = obj.instanceIndex;
    this.tag = 8;
  }
  serialize(): Uint8Array {
    return serialize(collectGarbageInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    splTokenProgram: PublicKey,
    marketAccount: PublicKey,
    instanceAccount: PublicKey,
    marketVault: PublicKey,
    marketSigner: PublicKey,
    targetQuoteAccount: PublicKey,
    memory_pages: PublicKey[]
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: splTokenProgram,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketSigner,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: targetQuoteAccount,
        isSigner: false,
        isWritable: true,
      },
    ];
    keys = keys.concat(
      memory_pages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class crankLiquidationInstruction {
  tag: number;
  instanceIndex: number;
  static schema: Schema = new Map([
    [
      crankLiquidationInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["instanceIndex", "u8"],
        ],
      },
    ],
  ]);

  constructor(obj: { instanceIndex: number }) {
    this.instanceIndex = obj.instanceIndex;
    this.tag = 9;
  }

  serialize(): Uint8Array {
    return serialize(crankLiquidationInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    splTokenProgram: PublicKey,
    marketAccount: PublicKey,
    instanceAccount: PublicKey,
    bonfida_bnb: PublicKey,
    marketVault: PublicKey,
    marketSigner: PublicKey,
    oracleAccount: PublicKey,
    targetQuoteAccount: PublicKey,
    memory_pages: PublicKey[]
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: splTokenProgram,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketSigner,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: bonfida_bnb,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: marketVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: targetQuoteAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: LIQUIDATION_LABEL,
        isSigner: false,
        isWritable: false,
      },
    ];
    keys = keys.concat(
      memory_pages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class crankFundingInstruction {
  tag: number;
  static schema: Schema = new Map([
    [
      crankFundingInstruction,
      {
        kind: "struct",
        fields: [["tag", "u8"]],
      },
    ],
  ]);

  constructor() {
    this.tag = 10;
  }
  serialize(): Uint8Array {
    return serialize(crankFundingInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    clockSysvarAccount: PublicKey,
    marketAccount: PublicKey,
    oracleAccount: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: clockSysvarAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: FUNDING_LABEL,
        isSigner: false,
        isWritable: false,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class extractFundingInstruction {
  tag: number;
  instanceIndex: number;
  static schema: Schema = new Map([
    [
      extractFundingInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["instanceIndex", "u8"],
        ],
      },
    ],
  ]);

  constructor(obj: { instanceIndex: number }) {
    this.tag = 11;
    this.instanceIndex = obj.instanceIndex;
  }
  serialize(): Uint8Array {
    return serialize(extractFundingInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    marketAccount: PublicKey,
    instanceAccount: PublicKey,
    userAccount: PublicKey,
    oracleAccount: PublicKey,
    memory_pages: PublicKey[]
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: marketAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: instanceAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: FUNDING_EXTRACTION_LABEL,
        isSigner: false,
        isWritable: false,
      },
      {
        pubkey: oracleAccount,
        isSigner: false,
        isWritable: false,
      },
    ];
    keys = keys.concat(
      memory_pages.map((m) => {
        return {
          pubkey: m,
          isSigner: false,
          isWritable: true,
        };
      })
    );

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class closeAccountInstruction {
  tag: number;
  static schema: Schema = new Map([
    [
      closeAccountInstruction,
      {
        kind: "struct",
        fields: [["tag", "u8"]],
      },
    ],
  ]);

  constructor() {
    this.tag = 13;
  }
  serialize(): Uint8Array {
    return serialize(closeAccountInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    userAccount: PublicKey,
    userAccountOwner: PublicKey,
    lamportsTarget: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: userAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: lamportsTarget,
        isSigner: false,
        isWritable: false,
      },
    ];
    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class transferUserAccountInstruction {
  tag: number;
  static schema: Schema = new Map([
    [
      withdrawBudgetInstruction,
      {
        kind: "struct",
        fields: [["tag", "u8"]],
      },
    ],
  ]);

  constructor() {
    this.tag = 16;
  }

  serialize(): Uint8Array {
    return serialize(withdrawBudgetInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    userAccount: PublicKey,
    userAccountOwner: PublicKey,
    newUserAccountOwner: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: userAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: userAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: newUserAccountOwner,
        isSigner: false,
        isWritable: false,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export class transferPositionInstruction {
  tag: number;
  positionIndex: number;
  static schema: Schema = new Map([
    [
      withdrawBudgetInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["positionIndex", "u16"],
        ],
      },
    ],
  ]);

  constructor(positionIndex: number) {
    this.tag = 17;
    this.positionIndex = positionIndex;
  }

  serialize(): Uint8Array {
    return serialize(withdrawBudgetInstruction.schema, this);
  }

  getInstruction(
    perpsProgramId: PublicKey,
    sourceUserAccount: PublicKey,
    sourceUserAccountOwner: PublicKey,
    destinationUserAccount: PublicKey,
    destinationUserAccountOwner: PublicKey
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys = [
      {
        pubkey: sourceUserAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: sourceUserAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: destinationUserAccountOwner,
        isSigner: true,
        isWritable: false,
      },
      {
        pubkey: destinationUserAccount,
        isSigner: false,
        isWritable: true,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: perpsProgramId,
      data,
    });
  }
}

export type PerpInstruction =
  | createMarketInstruction
  | addInstanceInstruction
  | openPositionInstruction
  | addBudgetInstruction
  | withdrawBudgetInstruction
  | increasePositionInstruction
  | closePositionInstruction
  | collectGarbageInstruction
  | crankLiquidationInstruction
  | crankFundingInstruction
  | extractFundingInstruction
  | closeAccountInstruction
  | transferUserAccountInstruction
  | transferPositionInstruction;

export type PerpTradeInstruction =
  | openPositionInstruction
  | increasePositionInstruction
  | closePositionInstruction;

export function parseInstructionData(buffer: Buffer): PerpInstruction {
  let types = [
    createMarketInstruction,
    addInstanceInstruction,
    updateOracleAccountInstruction,
    openPositionInstruction,
    addBudgetInstruction,
    withdrawBudgetInstruction,
    increasePositionInstruction,
    closePositionInstruction,
    collectGarbageInstruction,
    crankLiquidationInstruction,
    crankFundingInstruction,
    extractFundingInstruction,
    closeAccountInstruction,
    transferUserAccountInstruction,
    transferPositionInstruction,
  ];
  let t = types[buffer[0]];
  return deserializeUnchecked(t.schema, t, buffer);
}
