import { PublicKey, Connection } from "@solana/web3.js";
import BN from "bn.js";
import { Schema, deserializeUnchecked } from "borsh";
import { AccountLayout } from "@solana/spl-token";
import { PositionType } from "./instructions";

export enum StateTag {
  Uninitialized,
  MarketState,
  UserAccount,
  MemoryPage,
  Instance,
}

class PointerOption {
  pointer: number | undefined;
  constructor(obj: { pointer: number | undefined }) {
    this.pointer = obj.pointer;
  }
}

class PageInfo {
  uninitializedMemoryIndex: number;
  freeSlotListHd?: number;
  address: PublicKey;
  static size = 41;

  constructor(obj: {
    address: Uint8Array;
    uninitializedMemoryIndex: number;
    freeSlotListHd: PointerOption;
  }) {
    this.uninitializedMemoryIndex = obj.uninitializedMemoryIndex;
    this.address = new PublicKey(obj.address);
    this.freeSlotListHd = obj.freeSlotListHd.pointer;
  }
}

class Pages {
  pages: PageInfo[];

  constructor(obj: { pages: PageInfo[] }) {
    this.pages = obj.pages;
  }
}
export class Instance {
  shortsPointer?: number;
  longsPointer?: number;
  garbagePointer?: number;
  pages: PageInfo[];
  numberOfPages: number;
  static headerSize = 21;
  //@ts-ignore
  static schema: Schema = new Map([
    [
      Instance,
      {
        kind: "struct",
        fields: [
          ["version", "u8"],
          ["shortsPointer", PointerOption],
          ["longsPointer", PointerOption],
          ["garbagePointer", PointerOption],
          ["numberOfPages", "u32"],
        ],
      },
    ],
    [
      PointerOption,
      {
        kind: "enum",
        values: [
          ["None", [0]],
          ["pointer", "u32"],
        ],
      },
    ],
    [
      PageInfo,
      {
        kind: "struct",
        fields: [
          ["address", [32]],
          ["uninitializedMemoryIndex", "u32"],
          ["freeSlotListHd", PointerOption],
        ],
      },
    ],
    [
      Pages,
      {
        kind: "struct",
        fields: [["pages", [PageInfo]]],
      },
    ],
  ]);

  constructor(obj: {
    shortsPointer: PointerOption;
    longsPointer: PointerOption;
    garbagePointer: PointerOption;
    numberOfPages: number;
  }) {
    this.numberOfPages = obj.numberOfPages;
    this.pages = [];
    this.shortsPointer = obj.shortsPointer.pointer;
    this.longsPointer = obj.longsPointer.pointer;
    this.garbagePointer = obj.garbagePointer.pointer;
  }

  static async retrieve(
    connection: Connection,
    instanceAccount: PublicKey
  ): Promise<Instance> {
    let instanceData = await connection.getAccountInfo(
      instanceAccount,
      "processed"
    );
    if (instanceData === null) {
      throw new Error("Invalid instance account provided");
    }
    if (instanceData.data[0] !== StateTag.Instance) {
      throw new Error("The provided account isn't an instance account");
    }
    let res: Instance = deserializeUnchecked(
      this.schema,
      Instance,
      instanceData.data.slice(1, this.headerSize)
    );
    for (let i = 0; i < res.numberOfPages; i++) {
      let slice = instanceData.data.slice(this.headerSize + i * PageInfo.size);
      let page = deserializeUnchecked(this.schema, PageInfo, slice);
      res.pages.push(page);
    }
    return res;
  }
}

export class MarketState {
  marketAccount!: PublicKey;
  signerNonce: number;
  marketSymbol: string;
  oracleAddress: PublicKey;
  adminAddress: PublicKey;
  vaultAddress: PublicKey;
  quoteDecimals: number;
  coinDecimals: number;
  totalCollateral: number;
  totalUserBudgets: number;
  totalFeeBudget: number;
  rebalancingFunds: number;
  rebalancedVCoin: number;
  vCoinAmount: number;
  vQuoteAmount: number;
  openShortsVCoin: number;
  openLongsVCoin: number;
  openShortsVPc: number;
  openLongsVPc: number;
  lastFundingTimestamp: number;
  lastRecordingTimestamp: number;
  fundingSamplesCount: number;
  fundingSamplesSum: number;
  fundingHistoryOffset: number;
  fundingHistory: number[];
  fundingBalancingFactors: number[];
  instanceAddresses: PublicKey[];
  instances!: Instance[];
  static schema: Schema = new Map([
    [
      MarketState,
      {
        kind: "struct",
        fields: [
          ["version", "u8"],
          ["signerNonce", "u8"],
          ["marketSymbol", [32]],
          ["oracleAddress", [32]],
          ["adminAddress", [32]],
          ["vaultAddress", [32]],
          ["quoteDecimals", "u8"],
          ["coinDecimals", "u8"],
          ["totalCollateral", "u64"],
          ["totalUserBudgets", "u64"],
          ["totalFeeBudget", "u64"],
          ["rebalancingFunds", "u64"],
          ["rebalancedVCoin", "u64"],
          ["vCoinAmount", "u64"],
          ["vQuoteAmount", "u64"],
          ["openShortsVCoin", "u64"],
          ["openLongsVCoin", "u64"],
          ["openShortsVPc", "u64"],
          ["openLongsVPc", "u64"],
          ["lastFundingTimestamp", "u64"],
          ["lastRecordingTimestamp", "u64"],
          ["fundingSamplesCount", "u8"],
          ["fundingSamplesSum", "u64"],
          ["fundingHistoryOffset", "u8"],
          ["fundingHistory", [128]],
          ["fundingBalancingFactors", [128]],
          ["instanceAddresses", [[32]]],
        ],
      },
    ],
  ]);
  constructor(obj: {
    signerNonce: number;
    marketSymbol: Uint8Array;
    oracleAddress: Uint8Array;
    adminAddress: Uint8Array;
    vaultAddress: Uint8Array;
    quoteDecimals: number;
    coinDecimals: number;
    totalCollateral: BN;
    totalUserBudgets: BN;
    totalFeeBudget: BN;
    rebalancingFunds: BN;
    rebalancedVCoin: BN;
    vCoinAmount: BN;
    vQuoteAmount: BN;
    openShortsVCoin: BN;
    openLongsVCoin: BN;
    openShortsVPc: BN;
    openLongsVPc: BN;
    lastFundingTimestamp: BN;
    lastRecordingTimestamp: BN;
    fundingSamplesCount: number;
    fundingSamplesSum: BN;
    fundingHistoryOffset: number;
    fundingHistory: Uint8Array;
    fundingBalancingFactors: Uint8Array;
    instanceAddresses: Uint8Array[];
  }) {
    this.signerNonce = obj.signerNonce;
    this.marketSymbol = obj.marketSymbol.toString();
    this.oracleAddress = new PublicKey(obj.oracleAddress);
    this.adminAddress = new PublicKey(obj.adminAddress);
    this.vaultAddress = new PublicKey(obj.vaultAddress);
    this.quoteDecimals = obj.quoteDecimals;
    this.coinDecimals = obj.coinDecimals;
    this.totalCollateral = obj.totalCollateral.toNumber();
    this.vCoinAmount = obj.vCoinAmount.toNumber();
    this.vQuoteAmount = obj.vQuoteAmount.toNumber();
    this.openShortsVCoin = obj.openShortsVCoin.toNumber();
    this.openLongsVCoin = obj.openLongsVCoin.toNumber();
    this.openShortsVPc = obj.openShortsVPc.toNumber();
    this.openLongsVPc = obj.openShortsVCoin.toNumber();
    this.totalUserBudgets = obj.totalUserBudgets.fromTwos(64).toNumber();
    this.totalFeeBudget = obj.totalFeeBudget.toNumber();
    this.rebalancingFunds = obj.rebalancingFunds.toNumber();
    this.rebalancedVCoin = obj.rebalancedVCoin.fromTwos(64).toNumber();
    this.lastFundingTimestamp = obj.lastFundingTimestamp.toNumber();
    this.lastRecordingTimestamp = obj.lastRecordingTimestamp.toNumber();
    this.fundingSamplesCount = obj.fundingSamplesCount;
    this.fundingSamplesSum = obj.fundingSamplesSum.fromTwos(64).toNumber();
    this.fundingHistoryOffset = obj.fundingHistoryOffset;
    this.fundingHistory = [];
    this.fundingBalancingFactors = [];
    for (let i = 0; i < 16; i++) {
      let offset = 8 * i;
      this.fundingHistory.push(
        new BN(obj.fundingHistory.slice(offset, offset + 8), "le")
          .fromTwos(64)
          .toNumber()
      );
      this.fundingBalancingFactors.push(
        new BN(
          obj.fundingBalancingFactors.slice(offset, offset + 8),
          "le"
        ).toNumber()
      );
    }
    this.instanceAddresses = obj.instanceAddresses.map((s) => new PublicKey(s));
  }

  static async retrieve(
    connection: Connection,
    marketAccount: PublicKey
  ): Promise<MarketState> {
    let marketStateData = await connection.getAccountInfo(
      marketAccount,
      "processed"
    );
    if (!marketStateData) {
      throw new Error("Invalid market account provided");
    }
    if (marketStateData.data[0] !== StateTag.MarketState) {
      throw new Error("The provided account isn't a market account");
    }
    let res: MarketState = deserializeUnchecked(
      this.schema,
      MarketState,
      marketStateData.data.slice(1)
    );
    res.marketAccount = marketAccount;
    res.instances = await Promise.all(
      res.instanceAddresses.map((s) => Instance.retrieve(connection, s))
    );
    return res;
  }

  async getQuoteMint(connection: Connection): Promise<PublicKey> {
    let vaultInfo = await connection.getAccountInfo(
      this.vaultAddress,
      "processed"
    );
    if (vaultInfo === null) {
      throw new Error("Couldn't fetch the market vault's data");
    }
    return new PublicKey(AccountLayout.decode(vaultInfo.data).mint);
  }

  async getMarketSigner(programId: PublicKey): Promise<PublicKey> {
    return PublicKey.createProgramAddress(
      [this.marketAccount.toBuffer(), Buffer.from([this.signerNonce])],
      programId
    );
  }

  getMarkPrice() {
    return this.vQuoteAmount / this.vCoinAmount;
  }

  getFundingRatioLongShort() {
    let ratio = this.fundingSamplesSum / (this.fundingSamplesCount * 2 ** 32);
    if (ratio > 0) {
      let fundingRatioLongs = -ratio;
      let fundingRatioShorts = Math.min(
        (this.openLongsVCoin / this.openShortsVCoin) * ratio,
        ratio
      );
      return { fundingRatioLongs, fundingRatioShorts };
    } else {
      let fundingRatioShorts = ratio;
      let fundingRatioLongs = Math.min(
        (this.openShortsVCoin / this.openLongsVCoin) * -ratio,
        -ratio
      );
      return { fundingRatioLongs, fundingRatioShorts };
    }
  }

  getOpenInterest(): { shorts: number; longs: number } {
    return { shorts: this.openShortsVCoin, longs: this.openLongsVCoin };
  }

  getSlippageEstimation(side: PositionType, vPcAmount: number) {
    let markPrice = this.vQuoteAmount / this.vCoinAmount;
    let actualPrice;
    if (side == PositionType.Long) {
      actualPrice = (this.vQuoteAmount + vPcAmount) / this.vCoinAmount;
    } else {
      actualPrice = (this.vQuoteAmount - vPcAmount) / this.vCoinAmount;
    }
    return Math.abs(markPrice - actualPrice) / markPrice;
  }

  getLiquidationIndex(
    side: PositionType,
    vCoinAmount: number,
    collateral: number
  ) {
    let m = 1 / 20;
    let k = this.vCoinAmount * this.vQuoteAmount;
    let sideSign = side * 2 - 1;
    let py =
      (vCoinAmount / (this.vCoinAmount - vCoinAmount * sideSign)) *
      this.vQuoteAmount;
    let f = (py - sideSign * collateral) / (1 - sideSign * m);
    if (f <= 0) {
      return 0;
    }
    let liqIndex =
      (f ** 2 / (4 * k)) *
      (Math.sqrt(1 + (4 * k) / (f * vCoinAmount)) + sideSign) ** 2;
    return liqIndex;
  }
}

export class OpenPosition {
  static LEN = 43;
  side: PositionType;
  instanceIndex: number;
  lastFundingOffset: number;
  liquidationIndex: number;
  collateral: number;
  slotNumber: number;
  vCoinAmount: number;
  vPcAmount: number;

  constructor(obj: {
    lastFundingOffset: number;
    instanceIndex: number;
    side: number;
    liquidationIndex: BN;
    collateral: BN;
    slotNumber: BN;
    vCoinAmount: BN;
    vPcAmount: BN;
  }) {
    this.lastFundingOffset = obj.lastFundingOffset;
    this.instanceIndex = obj.instanceIndex;
    this.side = obj.side;
    this.liquidationIndex =
      obj.liquidationIndex.ushrn(32).toNumber() +
      obj.liquidationIndex.maskn(32).toNumber() / 2 ** 32;
    this.collateral = obj.collateral.toNumber();
    this.slotNumber = obj.slotNumber.toNumber();
    this.vCoinAmount = obj.vCoinAmount.toNumber();
    this.vPcAmount = obj.vPcAmount.toNumber();
  }
}

export class UserAccount {
  static LEN = 80;
  address!: PublicKey;
  owner: PublicKey;
  market: PublicKey;
  active: boolean;
  balance: number;
  lastFundingOffset: number;
  openPositions: OpenPosition[];

  //@ts-ignore
  static schema: Schema = new Map([
    [
      UserAccount,
      {
        kind: "struct",
        fields: [
          ["version", "u8"],
          ["owner", [32]],
          ["active", "u8"],
          ["market", [32]],
          ["balance", "u64"],
          ["lastFundingOffset", "u8"],
          ["openPositions", [OpenPosition]],
        ],
      },
    ],
    [
      OpenPosition,
      {
        kind: "struct",
        fields: [
          ["lastFundingOffset", "u8"],
          ["instanceIndex", "u8"],
          ["side", "u8"],
          ["liquidationIndex", "u64"],
          ["collateral", "u64"],
          ["slotNumber", "u64"],
          ["vCoinAmount", "u64"],
          ["vPcAmount", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    owner: Uint8Array;
    market: Uint8Array;
    active: number;
    balance: BN;
    lastFundingOffset: number;
    openPositions: OpenPosition[];
  }) {
    this.owner = new PublicKey(obj.owner);
    this.market = new PublicKey(obj.market);
    this.active = obj.active == 1;
    this.balance = obj.balance.toNumber();
    this.lastFundingOffset = obj.lastFundingOffset;
    this.openPositions = obj.openPositions;
  }

  static async retrieve(
    connection: Connection,
    account: PublicKey
  ): Promise<UserAccount> {
    let accountData = await connection.getAccountInfo(account, "processed");
    if (accountData === null) {
      throw new Error("Invalid market account provided");
    }
    return UserAccount.parse(account, accountData.data);
  }

  static parse(address: PublicKey, data: Buffer): UserAccount {
    if (data[0] !== StateTag.UserAccount) {
      throw new Error("The provided account isn't a user account");
    }
    let res: UserAccount = deserializeUnchecked(
      this.schema,
      UserAccount,
      data.slice(1)
    );
    res.address = address;
    return res;
  }
}
