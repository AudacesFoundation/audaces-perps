import { PublicKey } from "@solana/web3.js";
import { Schema } from "borsh";
export class MangoOracle {
  reward_amount: number;
  round_id: number;
  round_creation: number;
  round_update: number;
  answer_round_id: number;
  answer_median: number;
  answer_created_at: number;
  answer_updated_at: number;
  reward_token_account: PublicKey;
  owner: PublicKey;
  round_submissions: PublicKey;
  answer_submissions: PublicKey;
  description: string;
  decimals: number;
  restart_delay: number;
  max_submissions: number;
  min_submissions: number;
  is_initialized: boolean;

  static schema: Schema = new Map([
    [
      MangoOracle,
      {
        kind: "struct",
        fields: [
          ["description", [32]],
          ["decimals", "u8"],
          ["restart_delay", "u8"],
          ["max_submissions", "u8"],
          ["min_submissions", "u8"],
          ["reward_amount", "u64"],
          ["reward_token_account", [32]],
          ["is_initialized", "u8"],
          ["owner", [32]],
          ["round_id", "u64"],
          ["round_creation", "u64"],
          ["round_update", "u64"],
          ["round_submissions", [32]],
          ["answer_round_id", "u64"],
          ["answer_median", "u64"],
          ["answer_created_at", "u64"],
          ["answer_updated_at", "u64"],
          ["answer_submissions", [32]],
        ],
      },
    ],
  ]);
  constructor(obj) {
    let decoder = new TextDecoder("ascii");
    this.decimals = obj.decimals;
    this.restart_delay = obj.restart_delay;
    this.max_submissions = obj.max_submissions;
    this.min_submissions = obj.min_submissions;
    this.is_initialized = obj.is_initialized == 1;
    this.description = decoder.decode(obj.description);
    this.reward_amount = obj.reward_amount.toNumber();
    this.round_id = obj.round_id.toNumber();
    this.round_creation = obj.round_creation.toNumber();
    this.round_update = obj.round_update.toNumber();
    this.answer_round_id = obj.answer_round_id.toNumber();
    this.answer_median = obj.answer_median.toNumber();
    this.answer_created_at = obj.answer_created_at.toNumber();
    this.answer_updated_at = obj.answer_updated_at.toNumber();
    this.reward_token_account = new PublicKey(obj.reward_token_account);
    this.owner = new PublicKey(obj.owner);
    this.round_submissions = new PublicKey(obj.round_submissions);
    this.answer_submissions = new PublicKey(obj.answer_submissions);
  }
}
