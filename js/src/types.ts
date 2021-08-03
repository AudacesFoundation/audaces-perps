import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { OpenPosition } from "./state";
import { PerpInstruction } from "./instructions";

export interface Position {
  side: string;
  size: number;
  pnl: number;
  leverage: number;
  liqPrice: number;
  entryPrice: number;
  userAccount: PublicKey;
  collateral: number;
  marketAddress: PublicKey;
  positionIndex: number;
  vCoinAmount: number;
  instanceIndex: number;
}

export interface Order {
  market: PublicKey;
  userAccount: PublicKey;
  position: OpenPosition;
  position_index: number;
}

export interface Fees {
  total: number;
  refundable: number;
  fixed: number;
}
export interface PastInstruction {
  instruction: PerpInstruction;
  slot: number;
  time: number;
  tradeIndex: number;
  signature: string;
  log?: string[] | null | undefined;
  feePayer?: PublicKey | undefined;
  fees: Fees | undefined;
}

export interface PastTrade {
  instruction: PastInstruction;
  marketAddress: PublicKey | undefined;
  markPrice?: number;
  orderSize?: number;
  side?: number;
}
export interface PastInstructionRaw {
  signature: string;
  instruction: TransactionInstruction;
  slot: number;
  time: number;
  tradeIndex: number;
  log?: string[] | null | undefined;
  feePayer?: PublicKey | undefined;
}

export interface FundingDetails {
  funding: number;
  signature: string;
  fundingPayer: string;
}
