import {
  Keypair,
  Connection,
  PublicKey,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  TransactionInstruction,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, MintLayout, MintInfo } from "@solana/spl-token";
import {
  createAssociatedTokenAccount,
  findAssociatedTokenAddress,
  Numberu64,
} from "./utils";
import {
  addBudgetInstruction,
  addInstanceInstruction,
  BONFIDA_BNB,
  closeAccountInstruction,
  closePositionInstruction,
  collectGarbageInstruction,
  crankFundingInstruction,
  crankLiquidationInstruction,
  createMarketInstruction,
  extractFundingInstruction,
  increasePositionInstruction,
  openPositionInstruction,
  PositionType,
  transferPositionInstruction,
  transferUserAccountInstruction,
  withdrawBudgetInstruction,
} from "./instructions";
import { MarketState, OpenPosition, UserAccount } from "./state";
import BN from "bn.js";
import { getPriceAccountKey } from "./secondary_bindings";

///////////////////////////////////////////////////////

// mainnet;
export const PERPS_PROGRAM_ID = new PublicKey(
  "perpke6JybKfRDitCmnazpCrGN5JRApxxukhA9Js6E6"
);
// devnet
export const PYTH_MAPPING_ACCOUNT = new PublicKey(
  "AHtgzX45WTKfkPG53L6WYhGEXwQkN1BVknET3sVsLL8J"
);

export const CURRENT_RECOMMENDED_INSTANCE = 0;
const SLOT_SIZE = 33;
const DEFAULT_OPENPOSITIONS_CAPACITY = 128;
const MARKET_STATE_SPACE = 5000; // Size enough for more than 40 active leverage types with 10 memory pages each.

export type PrimedTransaction = [Keypair[], TransactionInstruction[]];

///////////////////////////////////////////////////////

/**
 * Returns a signers and transaction instructions to create a new Perpetual Futures AMM. The admin account will be the
 * centralized authority capable of creating new market instances. The signers array only contains the marketAccount to be
 * created.
 *
 * @param connection The solana connection object to the RPC node
 * @param adminAccount The market's administrative authority.
 * @param feePayer The address that will pay for this operation's transaction fees.
 * @param marketSymbol The symbol of the market that is to be created, example: "BTC/USD"
 * @param quoteMint The mint address of the market's base currency token
 * @param initial_v_quote_amount The initial amount of virtual quote currency.
 * @param vCoinDecimals The number of decimals which will be used in the market's internal vCoin representation.
 * @returns An array of signer accounts and an array of instructions. The admin account will need to sign the transaction.
 */
export async function createMarket(
  connection: Connection,
  adminAccount: PublicKey,
  feePayer: PublicKey,
  marketSymbol: string,
  quoteMint: PublicKey,
  vCoinDecimals: number,
  initial_v_quote_amount: Numberu64
): Promise<PrimedTransaction> {
  let balance = await connection.getMinimumBalanceForRentExemption(
    MARKET_STATE_SPACE
  );
  let marketAccount = new Keypair();
  let createMarketAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: balance,
    newAccountPubkey: marketAccount.publicKey,
    programId: PERPS_PROGRAM_ID,
    space: MARKET_STATE_SPACE,
  });

  let [vaultSigner, vaultSignerNonce] = await PublicKey.findProgramAddress(
    [marketAccount.publicKey.toBuffer()],
    PERPS_PROGRAM_ID
  );

  let quoteMintAccount = await connection.getAccountInfo(quoteMint);
  if (!quoteMintAccount) {
    throw "Could not retrieve quote mint account";
  }

  let quoteMintInfo: MintInfo = MintLayout.decode(quoteMintAccount.data);

  let marketVault = await findAssociatedTokenAddress(vaultSigner, quoteMint);

  let createVaultAccount = await createAssociatedTokenAccount(
    feePayer,
    vaultSigner,
    quoteMint
  );

  let oraclePriceAccount = await getPriceAccountKey(connection, marketSymbol);

  let createMarket = new createMarketInstruction({
    signerNonce: vaultSignerNonce,
    marketSymbol,
    initialVPcAmount: initial_v_quote_amount,
    coinDecimals: quoteMintInfo.decimals,
    quoteDecimals: vCoinDecimals,
  }).getInstruction(
    PERPS_PROGRAM_ID,
    marketAccount.publicKey,
    oraclePriceAccount,
    adminAccount,
    marketVault
  );

  let instructions = [createMarketAccount, createVaultAccount, createMarket];

  return [[marketAccount], instructions];
}
/**
 * This permissioned instruction has to be signed by the market administration key. It creates a new market instance. A particular market
 * can have multiple instances which will be cranked separately in order to allow for more scaling. Each instance has a set of memory pages
 * which adds up to its own memory
 *
 * @param connection The solana connection object to the RPC node
 * @param marketAccount The market's address.
 * @param marketAdmin The market's administrative authority.
 * @param feePayer The address that will pay for this operation's transaction fees.
 * @param numberOfPages The number of memory pages to allocate for this particular instance
 * @param pageSlots The size of the memory pages expressed in terms of memory slots (one slot = one tree node). In theory, the capacity
 * in terms of number of active positions is about half the number of slots. The limited speed of garbage collection means that it is necessary
 * to have a sufficient buffer of ready-to-use free memory at any time.
 * @returns An array of signer accounts and an array of instructions. The admin account will need to sign the transaction.
 */
export async function addInstance(
  connection: Connection,
  marketAccount: PublicKey,
  marketAdmin: PublicKey,
  feePayer: PublicKey,
  numberOfPages: number,
  pageSlots: number // size in slots
): Promise<PrimedTransaction> {
  let pageSize = 1 + pageSlots * SLOT_SIZE;
  let memoryPages: Keypair[] = [];
  for (let i = 0; i < numberOfPages; i++) {
    memoryPages.push(new Keypair());
  }

  let instanceAccount = new Keypair();
  let balance = await connection.getMinimumBalanceForRentExemption(
    MARKET_STATE_SPACE
  );

  let memoryBalance = await connection.getMinimumBalanceForRentExemption(
    pageSize
  );
  let instructions = memoryPages.map((m) =>
    SystemProgram.createAccount({
      fromPubkey: feePayer,
      newAccountPubkey: m.publicKey,
      lamports: memoryBalance,
      programId: PERPS_PROGRAM_ID,
      space: pageSize,
    })
  );
  instructions.push(
    SystemProgram.createAccount({
      fromPubkey: feePayer,
      newAccountPubkey: instanceAccount.publicKey,
      lamports: balance,
      programId: PERPS_PROGRAM_ID,
      space: MARKET_STATE_SPACE,
    })
  );

  let addLeverage = new addInstanceInstruction().getInstruction(
    PERPS_PROGRAM_ID,
    marketAccount,
    instanceAccount.publicKey,
    marketAdmin,
    memoryPages.map((m) => m.publicKey)
  );
  let signers = memoryPages;
  signers.push(instanceAccount);
  console.log(addLeverage.data);
  instructions.push(addLeverage);
  return [signers, instructions];
}

/**
 * Given a UserAccount with sufficient balance, this instruction allows for the opening of long and short position at current mark price with
 * some slippage incurred by the constant product curve of the vAMM.
 *
 *
 * @param connection The solana connection object to the RPC node
 * @param side Distinguishes between longs and shorts.
 * @param collateral The amount of collateral (in base currency) to be commited to this position
 * @param marketAddress The market's address
 * @param userAccountOwner The owner of the UserAccount
 * @param leverage The positions's leverage value (can be floating point number)
 * @param userAccount The user's account which holds a certain balance of base currency as well as a collection of open positions. The user's
 * balance periodically receives or pays funding for open positions with this balance.
 * @param discountAccount (optional) A FIDA token account. Sufficient FIDA stake allows access to more efficient fee levels.
 * @param discountAccountOwner (must be specified if discountAccount is specified). The owner of the FIDA discountAccount. This account must sign
 * the eventual openPosition transaction.
 * @param referrerAccount A referrer's account which will receive a fixed portion of protocol fees.
 * @returns An array of signer accounts and an array of instructions. The userAccountOwner account will need to sign the transaction.
 */
export async function openPosition(
  connection: Connection,
  side: PositionType,
  collateral: Numberu64,
  marketAddress: PublicKey,
  userAccountOwner: PublicKey,
  leverage: number,
  userAccount: PublicKey,
  discountAccount?: PublicKey,
  discountAccountOwner?: PublicKey,
  referrerAccount?: PublicKey,
  bonfida_bnb?: PublicKey,
  predictedEntryPrice?: number,
  maximumSlippageMargin?: number
): Promise<PrimedTransaction> {
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];
  let instanceIndex = CURRENT_RECOMMENDED_INSTANCE;
  if (!bonfida_bnb) {
    bonfida_bnb = BONFIDA_BNB;
  }

  let entryPrice;
  let slippage;

  if (!predictedEntryPrice) {
    entryPrice = new Numberu64(0);
    slippage = new Numberu64(new BN(0).notn(64));
  } else {
    entryPrice = new Numberu64(predictedEntryPrice * 2 ** 32);
    if (!maximumSlippageMargin) {
      throw new Error(
        "A slippage margin should be provided as well when giving an entry price"
      );
    }
    slippage = new Numberu64(maximumSlippageMargin * 2 ** 32);
  }

  let marketStateData = await connection.getAccountInfo(marketAddress);
  if (marketStateData === null) {
    throw new Error("Invalid market account provided");
  }
  let marketState = await MarketState.retrieve(connection, marketAddress);

  let vaultInfo = await connection.getAccountInfo(marketState.vaultAddress);
  if (vaultInfo === null) {
    throw new Error("Couldn't fetch the market vault's data");
  }

  let memoryPages = marketState.instances[instanceIndex].pages.map(
    (p) => p.address
  );
  instructions.push(
    new openPositionInstruction({
      side,
      collateral: collateral,
      instanceIndex,
      leverage: new Numberu64(leverage * 2 ** 32),
      predictedEntryPrice: entryPrice,
      maximumSlippageMargin: slippage,
    }).getInstruction(
      PERPS_PROGRAM_ID,
      marketAddress,
      marketState.instanceAddresses[instanceIndex],
      await marketState.getMarketSigner(PERPS_PROGRAM_ID),
      marketState.vaultAddress,
      userAccountOwner,
      userAccount,
      memoryPages,
      bonfida_bnb,
      marketState.oracleAddress,
      discountAccount,
      discountAccountOwner,
      referrerAccount
    )
  );
  return [signers, instructions];
}

/**
 * Allows for the partial closing and remodeling of positions. This operation allows the user to reduce or increase the leverage of
 * their position. The resulting leverage must not exceed the constraints defined by the margin ratio. It is recommended to keep a buffer
 * between the target position's leverage and this maximum value in order to avoid immediate liquidation which can incur a loss of funds.
 *
 * @param connection The solana connection object to the RPC node.
 * @param collateral The amount of collateral to extract from the position. This can be used to increase a position's leverage.
 * @param virtualCoin The amount of virtual coin to extract from the positions. This is used to reduce the position's size. A proportional amount of
 * collateral can be extracted as well in order to avoid reducing the position's leverage.
 * @param marketAddress The market's address
 * @param userAccountOwner The owner of the UserAccount
 * @param positionIndex The target position's index in the UserAccount
 * @param userAccount The user's account which holds a certain balance of base currency as well as a collection of open positions. The user's
 * balance periodically receives or pays funding for open positions with this balance.
 * @param discountAccount (optional) A FIDA token account. Sufficient FIDA stake allows access to more efficient fee levels.
 * @param discountAccountOwner (must be specified if discountAccount is specified). The owner of the FIDA discountAccount. This account must sign
 * the eventual openPosition transaction.
 * @param referrerAccount A referrer's account which will receive a fixed portion of protocol fees.
 * @returns An array of signer accounts and an array of instructions. The userAccountOwner account will need to sign the transaction.
 */
export async function closePosition(
  connection: Connection,
  collateral: Numberu64,
  virtualCoin: Numberu64,
  marketAddress: PublicKey,
  userAccountOwner: PublicKey,
  positionIndex: number,
  userAccount: PublicKey,
  discountAccount?: PublicKey,
  discountAccountOwner?: PublicKey,
  referrerAccount?: PublicKey,
  bonfida_bnb?: PublicKey,
  predictedEntryPrice?: number,
  maximumSlippageMargin?: number
): Promise<PrimedTransaction> {
  if (!bonfida_bnb) {
    bonfida_bnb = BONFIDA_BNB;
  }
  let marketState = await MarketState.retrieve(connection, marketAddress);

  let entryPrice;
  let slippage;

  if (!predictedEntryPrice) {
    entryPrice = new Numberu64(0);
    slippage = new Numberu64(new BN(0).notn(64));
  } else {
    entryPrice = new Numberu64(predictedEntryPrice * 2 ** 32);
    if (!maximumSlippageMargin) {
      throw new Error(
        "A slippage margin should be provided as well when giving an entry price"
      );
    }
    slippage = new Numberu64(maximumSlippageMargin * 2 ** 32);
  }

  let openPositionState = await UserAccount.retrieve(connection, userAccount);
  let instanceIndex =
    openPositionState.openPositions[positionIndex].instanceIndex;
  let memoryPages = marketState.instances[instanceIndex].pages.map(
    (p) => p.address
  );

  let instruction = new closePositionInstruction({
    positionIndex: Uint8Array.from(new BN(positionIndex).toArray("le", 2)),
    closingCollateral: collateral,
    closingVCoin: virtualCoin,
    predictedEntryPrice: entryPrice,
    maximumSlippageMargin: slippage,
  }).getInstruction(
    PERPS_PROGRAM_ID,
    await marketState.getMarketSigner(PERPS_PROGRAM_ID),
    marketState.vaultAddress,
    marketState.oracleAddress,
    marketAddress,
    marketState.instanceAddresses[instanceIndex],
    userAccountOwner,
    userAccount,
    memoryPages,
    bonfida_bnb,
    discountAccount,
    discountAccountOwner,
    referrerAccount
  );

  return [[], [instruction]];
}

/**
 * A permissionless crank which is used to perform memory garbage collection maintenance operations on the market. The cranker is
 * rewarded by a flat fee per freed memory slots.
 *
 * @param connection The solana connection object to the RPC node.
 * @param maxIterations The maximum number of slots to be freed in one instruction. A value too large will exceed the Solana compute budget.
 * @param marketAddress The market's address
 * @param targetFeeAccount A base token account which will receive the cranking reward.
 * @returns An array of signer accounts and an array of instructions.
 */
export async function collectGarbage(
  connection: Connection,
  maxIterations: number,
  marketAddress: PublicKey,
  targetFeeAccount: PublicKey // The token account associated to this target will receive the garbage collection fees
): Promise<PrimedTransaction> {
  let marketState = await MarketState.retrieve(connection, marketAddress);

  let quoteMint = await marketState.getQuoteMint(connection);

  let targetQuoteAccount = await findAssociatedTokenAddress(
    targetFeeAccount,
    quoteMint
  );

  let marketSigner = await marketState.getMarketSigner(PERPS_PROGRAM_ID);

  let instructions = marketState.instances.map((l, i) => {
    let memoryPages = l.pages.map((p) => p.address);
    return new collectGarbageInstruction({
      instanceIndex: i,
      maxIterations: new Numberu64(maxIterations),
    }).getInstruction(
      PERPS_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      marketAddress,
      marketState.instanceAddresses[i],
      marketState.vaultAddress,
      marketSigner,
      targetQuoteAccount,
      memoryPages
    );
  });

  return [[], instructions];
}

/**
 * A permissionless operation which will crank liquidation operations for the market. This operation is run at high frequencies in order to
 * increase market reliability.
 *
 * @param connection The solana connection object to the RPC node.
 * @param targetFeeAccount A base token account which will receive the cranking reward.
 * @param marketAddress The market's address
 * @returns An array of signer accounts and an array of instructions
 */
export async function crankLiquidation(
  connection: Connection,
  targetFeeAccount: PublicKey,
  marketAddress: PublicKey,
  bonfida_bnb?: PublicKey
): Promise<PrimedTransaction> {
  let marketState = await MarketState.retrieve(connection, marketAddress);
  let quoteMint = await marketState.getQuoteMint(connection);

  let targetQuoteAccount = await findAssociatedTokenAddress(
    targetFeeAccount,
    quoteMint
  );

  let marketSigner = await marketState.getMarketSigner(PERPS_PROGRAM_ID);

  let instructions = marketState.instances.map((l, i) => {
    let memoryPages = l.pages.map((p) => p.address);
    return new crankLiquidationInstruction({ instanceIndex: i }).getInstruction(
      PERPS_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      marketAddress,
      marketState.instanceAddresses[i],
      bonfida_bnb ? bonfida_bnb : BONFIDA_BNB,
      marketState.vaultAddress,
      marketSigner,
      marketState.oracleAddress,
      targetQuoteAccount,
      memoryPages
    );
  });

  return [[], instructions];
}

/**
 * A permissionless operation which will crank funding operations for the market. This operation is run at a frequency higher than the funding
 * frequency in order to maintain an estimator of the funding ratio mean for the current funding period.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The market's address
 * @returns An array of signer accounts and an array of instructions.
 */
export async function crankFunding(
  connection: Connection,
  marketAddress: PublicKey
): Promise<PrimedTransaction> {
  let marketState = await MarketState.retrieve(connection, marketAddress);

  let instructions = [
    new crankFundingInstruction().getInstruction(
      PERPS_PROGRAM_ID,
      SYSVAR_CLOCK_PUBKEY,
      marketAddress,
      marketState.oracleAddress
    ),
  ];

  return [[], instructions];
}

/**
 * This operation is used to create UserAccount and to add a balance of base tokens. The account is used to store all currently open
 * positions on the market. It is very important to keep this account sufficiently funded when it contains open positions since
 * its balance is used to perform and receive funding payments. An insufficient balance can yield to a liquidation of all open positions
 * and loss of funds.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The market's address
 * @param amount The amount of base tokens to add to the account.
 * @param feePayer The address that will pay for this operation's transaction fees.
 * @param sourceQuoteAccount The source base token account.
 * @param sourceOwnerAccount The owner of the source base token account.
 * @param userAccount (optional) The user account's address. When left undefined, a new user account is created.
 * @param instanceIndex (optional) The index number of the market instance which should be used when creating the account. This
 * paramater is ignored when not creating a new account.
 * @returns An array of signer accounts and an array of instructions. When creating a new user account, the first signer is this new user account.
 */
export async function addBudget(
  connection: Connection,
  marketAddress: PublicKey,
  amount: number,
  feePayer: PublicKey,
  sourceQuoteAccount: PublicKey,
  sourceOwnerAccount: PublicKey,
  userAccount?: PublicKey
): Promise<PrimedTransaction> {
  let marketState = await MarketState.retrieve(connection, marketAddress);
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];

  if (!userAccount) {
    let userAccountKeypair = new Keypair();
    console.log("Open Positions: ", userAccountKeypair.publicKey.toBase58());
    userAccount = userAccountKeypair.publicKey;
    let size =
      UserAccount.LEN + OpenPosition.LEN * DEFAULT_OPENPOSITIONS_CAPACITY;
    let balance = await connection.getMinimumBalanceForRentExemption(size);
    instructions.push(
      SystemProgram.createAccount({
        fromPubkey: feePayer,
        newAccountPubkey: userAccount,
        lamports: balance,
        space: size,
        programId: PERPS_PROGRAM_ID,
      })
    );
    signers.push(userAccountKeypair);
  }

  let instruction = new addBudgetInstruction({
    amount: new Numberu64(amount),
  }).getInstruction(
    PERPS_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    marketAddress,
    marketState.vaultAddress,
    sourceQuoteAccount,
    sourceOwnerAccount,
    userAccount
  );
  instructions.push(instruction);

  return [signers, instructions];
}

/**
 * This operation is used to withdraw funds from a UserAccount. Users should be careful when withdrawing funds to leave enough for potential
 * funding payments on open positions.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The market's address
 * @param amount The amount of base tokens to be withdrawn from the account.
 * @param targetQuoteAccount The target base token account which will receive the withdrawn funds.
 * @param userAccountOwner The owner of the user account. This account will need to sign the eventual transaction.
 * @param userAccount The user account's address.
 * @returns An array of signer accounts and an array of instructions. The user account owner should sign the resulting transaction.
 */
export async function withdrawBudget(
  connection: Connection,
  marketAddress: PublicKey,
  amount: number,
  targetQuoteAccount: PublicKey,
  userAccountOwner: PublicKey,
  userAccount: PublicKey
): Promise<PrimedTransaction> {
  let marketState = await MarketState.retrieve(connection, marketAddress);
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];

  let instruction = new withdrawBudgetInstruction({
    amount: new Numberu64(amount),
  }).getInstruction(
    PERPS_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    marketAddress,
    marketState.vaultAddress,
    targetQuoteAccount,
    await marketState.getMarketSigner(PERPS_PROGRAM_ID),
    userAccount,
    userAccountOwner
  );
  instructions.push(instruction);

  return [signers, instructions];
}

/**
 * This operations allows user to increase a position's collateral or size, which can affect the position's leverage.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The market's address
 * @param addCollateral The amount of collateral to add to the position. This can be used to decrease the position's leverage.
 * @param leverage A non-zero value is used to increase the position's size. The resulting increase in position size will be equal to addCollateral * leverage.
 * @param positionIndex The index of the target position in the user's account.
 * @param userAccountOwner The owner of the user account.
 * @param userAccount The user account.
 * @param discountAccount (optional) A FIDA token account. Sufficient FIDA stake allows access to more efficient fee levels.
 * @param discountAccountOwner (must be specified if discountAccount is specified). The owner of the FIDA discountAccount. This account must sign
 * the eventual openPosition transaction.
 * @param referrerAccount A referrer's account which will receive a fixed portion of protocol fees.
 * @returns An array of signer accounts and an array of instructions. The user account owner should sign the resulting transaction.
 */
export async function increasePosition(
  connection: Connection,
  marketAddress: PublicKey,
  addCollateral: number,
  leverage: number,
  positionIndex: number,
  userAccountOwner: PublicKey,
  userAccount: PublicKey,
  bonfida_bnb?: PublicKey,
  discountAccount?: PublicKey,
  discountAccountOwner?: PublicKey,
  referrerAccount?: PublicKey,
  predictedEntryPrice?: number,
  maximumSlippageMargin?: number
): Promise<PrimedTransaction> {
  if (!bonfida_bnb) {
    bonfida_bnb = BONFIDA_BNB;
  }
  let marketState = await MarketState.retrieve(connection, marketAddress);
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];
  let openPositionState = await UserAccount.retrieve(connection, userAccount);
  let instanceIndex =
    openPositionState.openPositions[positionIndex].instanceIndex;
  let memoryPages = marketState.instances[instanceIndex].pages.map(
    (p) => p.address
  );
  let entryPrice;
  let slippage;

  if (!predictedEntryPrice) {
    entryPrice = new Numberu64(0);
    slippage = new Numberu64(new BN(0).notn(64));
  } else {
    entryPrice = new Numberu64(predictedEntryPrice * 2 ** 32);
    if (!maximumSlippageMargin) {
      throw new Error(
        "A slippage margin should be provided as well when giving an entry price"
      );
    }
    slippage = new Numberu64(maximumSlippageMargin * 2 ** 32);
  }

  let instruction = new increasePositionInstruction({
    addCollateral: new Numberu64(addCollateral),
    instanceIndex,
    leverage: new Numberu64(leverage * 2 ** 32),
    positionIndex: Uint8Array.from(new BN(positionIndex).toArray("le", 2)),
    predictedEntryPrice: entryPrice,
    maximumSlippageMargin: slippage,
  }).getInstruction(
    PERPS_PROGRAM_ID,
    SYSVAR_CLOCK_PUBKEY,
    marketAddress,
    await marketState.getMarketSigner(PERPS_PROGRAM_ID),
    marketState.vaultAddress,
    bonfida_bnb,
    marketState.instanceAddresses[instanceIndex],
    userAccount,
    userAccountOwner,
    marketState.oracleAddress,
    memoryPages,
    discountAccount,
    discountAccountOwner,
    referrerAccount
  );
  instructions.push(instruction);

  return [signers, instructions];
}

/**
 * This operation is a permissionless crank used to extract or inject funding from/into all user accounts. This operation is run on all active accounts
 * within any funding period.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The market's address
 * @param userAccount The user account.
 * @returns An array of signer accounts and an array of instructions.
 */
export async function fundingExtraction(
  connection: Connection,
  marketAddress: PublicKey,
  instanceIndex: number,
  userAccount: PublicKey
): Promise<PrimedTransaction> {
  let marketState = await MarketState.retrieve(connection, marketAddress);
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];
  let memoryPages = marketState.instances[instanceIndex].pages.map(
    (p) => p.address
  );

  let instruction = new extractFundingInstruction({
    instanceIndex: instanceIndex,
  }).getInstruction(
    PERPS_PROGRAM_ID,
    marketAddress,
    marketState.instanceAddresses[instanceIndex],
    userAccount,
    marketState.oracleAddress,
    memoryPages
  );
  instructions.push(instruction);

  return [signers, instructions];
}
/**
 * This operation closes a user account and should be signed by its owner. The user account should have no
 * open positions and no remaining balance in order for this operation to succeed.
 *
 * @param userAccount The user account.
 * @param userAccountOwner The owner of the user account.
 * @param lamportsTarget The account which will receive the closed account's lamports (SOL).
 * @returns An array of signer accounts and an array of instructions.
 */
export async function closeAccount(
  userAccount: PublicKey,
  userAccountOwner: PublicKey,
  lamportsTarget: PublicKey
): Promise<PrimedTransaction> {
  let instruction = new closeAccountInstruction().getInstruction(
    PERPS_PROGRAM_ID,
    userAccount,
    userAccountOwner,
    lamportsTarget
  );

  return [[], [instruction]];
}

/**
 * This operation is used to transfer the ownership of a UserAccount to a new address.
 *
 * @param userAccountOwner The owner of the user account. This account will need to sign the eventual transaction.
 * @param userAccount The user account's address.
 * @param newUserAccountOwner The new owner of the user account.
 * @returns An array of signer accounts and an array of instructions. The user account owner should sign the resulting transaction.
 */
export async function transferUserAccount(
  userAccountOwner: PublicKey,
  userAccount: PublicKey,
  newUserAccountOwner: PublicKey
): Promise<PrimedTransaction> {
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];

  let instruction = new transferUserAccountInstruction().getInstruction(
    PERPS_PROGRAM_ID,
    userAccount,
    userAccountOwner,
    newUserAccountOwner
  );
  instructions.push(instruction);

  return [signers, instructions];
}

/**
 * This operation is used to transfer a position from a UserAccount to another.
 *
 * @param sourceUserAccountOwner The owner of the source user account. This account will need to sign the eventual transaction.
 * @param sourceUserAccount The source user account's address.
 * @param destinationUserAccountOwner The owner of the destination user account. This account will need to sign the eventual transaction.
 * @param destinationUserAccount The source user account's address.
 * @returns An array of signer accounts and an array of instructions. The user account owner should sign the resulting transaction.
 */
export async function transferPosition(
  positionIndex: number,
  sourceUserAccountOwner: PublicKey,
  sourceUserAccount: PublicKey,
  destinationUserAccountOwner: PublicKey,
  destinationUserAccount: PublicKey
): Promise<PrimedTransaction> {
  let instructions: TransactionInstruction[] = [];
  let signers: Keypair[] = [];

  let instruction = new transferPositionInstruction(
    positionIndex
  ).getInstruction(
    PERPS_PROGRAM_ID,
    sourceUserAccount,
    sourceUserAccountOwner,
    destinationUserAccount,
    destinationUserAccountOwner
  );
  instructions.push(instruction);

  return [signers, instructions];
}
