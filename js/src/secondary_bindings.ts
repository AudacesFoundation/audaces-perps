import {
  parseMappingData,
  parsePriceData,
  parseProductData,
} from "@pythnetwork/client";
import {
  ConfirmedSignaturesForAddress2Options,
  ConfirmedTransaction,
  Connection,
  PublicKey,
} from "@solana/web3.js";
import { deserialize } from "borsh";
import { PERPS_PROGRAM_ID, PYTH_MAPPING_ACCOUNT } from "./bindings";

import {
  parseInstructionData,
  PositionType,
  LIQUIDATION_LABEL,
  TRADE_LABEL,
  FUNDING_EXTRACTION_LABEL,
} from "./instructions";
import { MangoOracle } from "./oracle_utils";
import { MarketState, UserAccount } from "./state";
import { getFilteredProgramAccounts } from "./utils";
import {
  Order,
  PastInstruction,
  PastTrade,
  PastInstructionRaw,
  Fees,
  FundingDetails,
} from "./types";

export async function getOrders(connection: Connection, owner: PublicKey) {
  let orders: Order[] = [];
  const accounts = await getUserAccounts(connection, owner);
  for (let p of accounts) {
    try {
      let parsed = UserAccount.parse(p.publicKey, p.accountInfo.data);
      let market = parsed.market;
      for (let idx = 0; idx < parsed.openPositions.length; idx++) {
        orders.push({
          userAccount: p.publicKey,
          position: parsed.openPositions[idx],
          position_index: idx,
          market,
        });
      }
    } catch (err) {
      console.log(
        `Found corrupted UserAccount at,${p.publicKey.toBase58()} - error ${err} - Skipping.`
      );
    }
  }
  return orders;
}

async function getUserAccounts(connection: Connection, owner: PublicKey) {
  const filters = [
    {
      memcmp: {
        offset: 2,
        bytes: owner.toBase58(),
      },
    },
  ];
  return await getFilteredProgramAccounts(
    connection,
    PERPS_PROGRAM_ID,
    filters
  );
}

export async function getUserAccountsForOwner(
  connection: Connection,
  owner: PublicKey
): Promise<(UserAccount | undefined)[]> {
  return (await getUserAccounts(connection, owner)).map((p) => {
    console.log(p.publicKey.toBase58());
    try {
      return UserAccount.parse(p.publicKey, p.accountInfo.data);
    } catch {
      console.log(
        "Found corrupted UserAccount at ",
        p.publicKey.toBase58(),
        ". Skipping."
      );
    }
  });
}

export async function getMarketState(
  connection: Connection,
  marketAddress: PublicKey
) {
  return await MarketState.retrieve(connection, marketAddress);
}

// This applies to the Pyth Oracle only
export async function getOraclePrice(
  connection: Connection,
  oraclePriceAccountAddress: PublicKey
) {
  let oracle_data = (await connection.getAccountInfo(oraclePriceAccountAddress))
    ?.data;
  if (!oracle_data) {
    throw "Unable to retrieve oracle data";
  }

  let { price, confidence } = parsePriceData(oracle_data);

  return { price, confidence };
}

// This applies to the Pyth Oracle only
export async function getPriceAccountKey(
  connection: Connection,
  marketSymbol: string
) {
  let mappingAccountInfo = await connection.getAccountInfo(
    PYTH_MAPPING_ACCOUNT
  );
  if (!mappingAccountInfo) {
    throw "Unable to retrieve mapping oracle data";
  }
  const { productAccountKeys } = parseMappingData(mappingAccountInfo.data);
  for (let k of productAccountKeys) {
    let productAccountInfo = await connection.getAccountInfo(k);
    if (!productAccountInfo) {
      throw "Unable to retrieve product oracle data";
    }
    const { product, priceAccountKey } = parseProductData(
      productAccountInfo.data
    );

    if (product["symbol"] == marketSymbol) {
      return priceAccountKey;
    }
  }
  throw "Could not find the requested symbol.";
}

export async function getMangoOraclePrice(
  connection: Connection,
  oracleAddress: PublicKey
) {
  let oracle_data = (await connection.getAccountInfo(oracleAddress))?.data;
  if (!oracle_data) {
    throw "Unable to retrieve oracle data";
  }

  let oracle: MangoOracle = deserialize(
    MangoOracle.schema,
    MangoOracle,
    oracle_data
  );

  return oracle.answer_median / 10 ** oracle.decimals;
}
export async function getLiquidationTransaction(
  connection: Connection,
  closeSignature: string,
  marketAddress: PublicKey,
  side: PositionType
) {
  let re =
    /Program log: Order not found, it was liquidated at index: (?<liquidationIndex>.*), with collateral (?<collateral>.*), with parent node slot (?<parentNodeSlot>.*)/;
  let tx = await connection.getParsedConfirmedTransaction(
    closeSignature,
    "confirmed"
  );
  let logMessages = tx?.meta?.logMessages;
  if (!logMessages) {
    throw "Failed to parse transaction";
  }
  let liquidationIndex: number = 0;
  for (let l of logMessages) {
    let m = l.match(re);
    if (!!m) {
      liquidationIndex = parseInt((m.groups as any)["liquidationIndex"]);
    }
  }
  let side_sign = side * 2 - 1;
  let re2 = /Program log: Liquidation index: (?<liquidationIndex>.*)/;
  let lastSignature = closeSignature;
  while (true) {
    let txs = await getPastInstructions(
      connection,
      LIQUIDATION_LABEL,
      marketAddress,
      {
        before: lastSignature,
      }
    );
    lastSignature = txs[txs.length - 1].signature;
    for (let t of txs) {
      let logMessages = t.log as string[];
      for (let l of logMessages) {
        let m = l.match(re2);
        if (!!m) {
          let actualLiquidationIndex = parseInt(
            (m.groups as any)["liquidationIndex"]
          );
          if ((liquidationIndex - actualLiquidationIndex) * side_sign >= 0) {
            // Found a liquidation transaction which proves that the position has been liquidated.
            return t.signature;
          }
        }
      }
    }
  }
}

export async function getPastInstructions(
  connection: Connection,
  lookupAddress: PublicKey,
  marketAddress: PublicKey,
  options?: ConfirmedSignaturesForAddress2Options
): Promise<PastInstruction[]> {
  let sigs = await connection.getConfirmedSignaturesForAddress2(
    lookupAddress,
    options,
    "confirmed"
  );
  console.log("Retrieved signatures: ", sigs.length);
  let pastInstructions: PastInstruction[] = (
    await getPastInstructionsRaw(
      connection,
      lookupAddress,
      marketAddress,
      options
    )
  ).map(parseRawInstruction);

  return pastInstructions;
}

const getFees = (logs: string[] | null | undefined) => {
  if (!logs) return;
  const regex =
    /Fees : Fees { total: (?<total>.*), refundable: (?<refundable>.*), fixed: (?<fixed>.*) }/;
  for (let log of logs) {
    let result = log.match(regex);
    if (!!result?.groups) {
      let fees: Fees;
      fees = {
        total: parseInt(result.groups.total),
        refundable: parseInt(result.groups.refundable),
        fixed: parseInt(result.groups.fixed),
      };
      return fees;
    }
  }
};

async function getPastInstructionsRaw(
  connection: Connection,
  lookupAddress: PublicKey,
  marketAddress: PublicKey,
  options?: ConfirmedSignaturesForAddress2Options
): Promise<PastInstructionRaw[]> {
  let sigs = await connection.getConfirmedSignaturesForAddress2(
    lookupAddress,
    options,
    "confirmed"
  );
  console.log("Retrieved signatures: ", sigs.length);
  let pastInstructions: PastInstructionRaw[] = [];

  let i = 0;

  for (let s of sigs) {
    console.log("Retrieving ", i, " with sig ", s.signature);
    i++;
    let tx_null = await connection.getConfirmedTransaction(
      s.signature,
      "confirmed"
    );
    if (!tx_null || !!tx_null?.meta?.err) {
      continue;
    }
    let tx = tx_null as ConfirmedTransaction;
    let tradeIndex = 0;
    tx.transaction.instructions.forEach((i) => {
      if (i.programId.toBase58() !== PERPS_PROGRAM_ID.toBase58()) {
        console.log("skipped with programId: ", i.programId.toBase58());
        return;
      }
      if (
        !i.keys.find((k) => {
          return k.pubkey.toBase58() === marketAddress.toBase58();
        })
      ) {
        console.log("Skipped irrelevant instruction");
        // This instruction is irrelevant to the current market
        return;
      }
      let log = tx.meta?.logMessages;
      pastInstructions.push({
        signature: s.signature,
        instruction: i,
        time: tx.blockTime as number,
        slot: tx.slot,
        log,
        feePayer: tx.transaction.feePayer,
        tradeIndex,
      });
      if (i.data[0] in [2, 5, 6]) {
        tradeIndex++;
      }
    });
  }
  console.log("Retrieved raw past instructions");
  return pastInstructions;
}

export async function getPastTrades(
  connection: Connection,
  marketAddress: PublicKey,
  options?: ConfirmedSignaturesForAddress2Options
): Promise<PastTrade[]> {
  let pastInstructions = await getPastInstructionsRaw(
    connection,
    TRADE_LABEL,
    marketAddress,
    options
  );
  console.log("Unfiltered length: ", pastInstructions.length);
  let filtered = pastInstructions.filter((i) => {
    return (
      [3, 6, 7].includes(i.instruction.data[0]) &&
      i.instruction.programId.toBase58() === PERPS_PROGRAM_ID.toBase58()
    );
  });
  console.log("Filtered length: ", filtered.length);
  let parsed = filtered
    .map(parseRawInstruction)
    .map(extractTradeInfo)
    .map((t) => {
      t.marketAddress = marketAddress;
      return t;
    });
  return parsed;
}

function parseRawInstruction(i: PastInstructionRaw): PastInstruction {
  return {
    instruction: parseInstructionData(i.instruction.data),
    slot: i.slot,
    time: i.time,
    log: i.log,
    feePayer: i.feePayer,
    tradeIndex: i.tradeIndex,
    signature: i.signature,
    fees: getFees(i.log),
  };
}

export async function extractTradeInfoFromTransaction(
  connection: Connection,
  txSig: string
): Promise<PastTrade[]> {
  let tx_null = await connection.getConfirmedTransaction(txSig, "confirmed");
  if (!tx_null) {
    throw "Could not retrieve transaction";
  }
  let tx = tx_null as ConfirmedTransaction;
  let instructions = tx_null.transaction.instructions.filter(
    (i) => i.programId.toBase58() === PERPS_PROGRAM_ID.toBase58()
  );
  instructions = instructions.filter((v) => [3, 6, 7].includes(v.data[0]));
  let parsedInstructions: PastInstructionRaw[] = instructions.map((v, idx) => {
    return {
      signature: txSig,
      instruction: v,
      time: tx.blockTime as number,
      slot: tx.slot,
      log: tx.meta?.logMessages,
      feePayer: tx.transaction.feePayer,
      tradeIndex: idx,
    };
  });
  return parsedInstructions.map(parseRawInstruction).map(extractTradeInfo);
}

const markPriceExtractionRe =
  /Program log: Mark price for this transaction \(FP32\): (?<markPrice>.*), with size: (?<orderSize>.*) and side (?<side>.*)/;

function extractTradeInfo(i: PastInstruction): PastTrade {
  if (!i.log) {
    throw "Unable to parse mark price due to empty log";
  }
  let currentTradeIndex = 0;
  let markPrice: number | undefined;
  let orderSize: number | undefined;
  let side: number | undefined;
  for (let l of i.log) {
    let results = l.match(markPriceExtractionRe);
    if (!results) {
      continue;
    }
    if (currentTradeIndex === i.tradeIndex) {
      let markPriceStr = results.groups?.markPrice as string;
      let orderSizeStr = results.groups?.orderSize as string;
      let sideStr = results.groups?.side as string;
      markPrice = parseInt(markPriceStr) / 2 ** 32;
      orderSize = parseInt(orderSizeStr) / Math.pow(10, 6);
      if (sideStr === "Long") {
        side = PositionType.Long;
      } else {
        side = PositionType.Short;
      }
    }
    currentTradeIndex++;
  }
  return {
    instruction: i,
    marketAddress: undefined,
    markPrice,
    orderSize,
    side,
  };
}

const fundingRegex =
  /Program log: Extracting (?<funding>.*) from user account for funding/;

export async function getFundingPaymentsHistoryForUser(
  connection: Connection,
  userAccount: PublicKey,
  marketAddress: PublicKey,
  options?: ConfirmedSignaturesForAddress2Options
): Promise<FundingDetails[]> {
  let txs = await connection.getConfirmedSignaturesForAddress2(
    userAccount,
    options,
    "confirmed"
  );
  let fundingSignatures = [] as FundingDetails[];

  for (let t of txs) {
    let details = await connection.getConfirmedTransaction(
      t.signature,
      "confirmed"
    );
    let instructions = details?.transaction.instructions;
    if (!instructions) {
      continue;
    }
    let fundingPayer = "";
    let containsFunding = false;
    for (let i of instructions) {
      if (
        i.programId.toBase58() !== PERPS_PROGRAM_ID.toBase58() ||
        i.data[0] != 11 ||
        i.keys[0].pubkey.toBase58() !== marketAddress.toBase58()
      ) {
        continue;
      }
      fundingPayer = i.keys[2].pubkey.toBase58(); // User account paying the funding
      containsFunding = true;
      break;
    }
    let fundingExtracted: number | undefined = undefined;
    if (!details?.meta?.logMessages) {
      continue;
    }
    for (let log of details?.meta?.logMessages) {
      const results = log?.match(fundingRegex);
      if (!results || !results?.groups?.funding) {
        continue;
      }
      fundingExtracted = parseFloat(results?.groups?.funding);
    }

    if (containsFunding && !!fundingExtracted) {
      fundingSignatures.push({
        funding: fundingExtracted,
        signature: t.signature,
        fundingPayer: fundingPayer,
      });
    }
  }
  return fundingSignatures;
}

export async function getFundingPaymentsHistory(
  connection: Connection,
  marketAddress: PublicKey,
  options?: ConfirmedSignaturesForAddress2Options
): Promise<FundingDetails[]> {
  let txs = await connection.getConfirmedSignaturesForAddress2(
    FUNDING_EXTRACTION_LABEL,
    options,
    "confirmed"
  );
  let crankedFundingSignatures = [] as FundingDetails[];
  for (let t of txs) {
    let details = await connection.getConfirmedTransaction(
      t.signature,
      "confirmed"
    );
    let instructions = details?.transaction.instructions;
    if (!instructions) {
      console.log("Empty instructions");
      continue;
    }
    let fundingPayer = "";
    let containsCrankFunding = false;
    for (let i of instructions) {
      if (
        i.programId.toBase58() !== PERPS_PROGRAM_ID.toBase58() ||
        i.data[0] != 11 ||
        i.keys[0].pubkey.toBase58() !== marketAddress.toBase58()
      ) {
        console.log("Skpping instruction");
        continue;
      }

      containsCrankFunding = true;
      fundingPayer = i.keys[2].pubkey.toBase58(); // User account paying the funding
      break;
    }
    let fundingExtracted: number | undefined = undefined;
    if (!details?.meta?.logMessages) {
      continue;
    }
    for (let log of details?.meta?.logMessages) {
      const results = log?.match(fundingRegex);
      if (!results || !results?.groups?.funding) {
        continue;
      }
      fundingExtracted = parseFloat(results?.groups?.funding);
    }

    if (containsCrankFunding && !!fundingExtracted) {
      crankedFundingSignatures.push({
        funding: fundingExtracted,
        signature: t.signature,
        fundingPayer: fundingPayer,
      });
    }
  }
  return crankedFundingSignatures;
}
