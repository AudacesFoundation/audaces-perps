// More user friendly than the ones in bindings.ts and secondary_bindings.ts
import { Connection, PublicKey, Keypair } from "@solana/web3.js";
import {
  addBudget,
  PrimedTransaction,
  withdrawBudget,
  closePosition,
  increasePosition,
  openPosition,
} from "./bindings";
import {
  findAssociatedTokenAddress,
  signAndSendTransactionInstructions,
  createAssociatedTokenAccount,
  Numberu64,
} from "./utils";
import {
  extractTradeInfoFromTransaction,
  getMarketState,
  getUserAccountsForOwner,
  getPastTrades,
  getOrders,
} from "./secondary_bindings";
import { PositionType } from "./instructions";
import { UserAccount } from "./state";
import { Position } from "./types";

const Cache = new Map<string, PublicKey>();

enum Env {
  dev = "dev",
  prod = "prod",
}

export const BNB_ADDRESS = new PublicKey(
  "4qZA7RixzEgQ53cc6ittMeUtkaXgCnjZYkP8L1nxFD25"
);

// Assumes the wallet used for trading has an associated token account for USDC
// If it's not the case run the following initWallet function
const USDC_MINT = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

// FIDA token mint
const FIDA_MINT = new PublicKey("EchesyfXePKdLtoiZSL8pBe8Myagyy8ZRqsACNCFGnvp");

/**
 * Creates an associated USDC account for a wallet.
 *
 * @param connection The solana connection object to the RPC node.
 * @param wallet The wallet that does not have an associated USDC account. Needs to sign.
 * @returns The signature of the transaction.
 */
export const initWallet = async (connection: Connection, wallet: Keypair) => {
  const instruction = await createAssociatedTokenAccount(
    wallet.publicKey,
    wallet.publicKey,
    USDC_MINT
  );
  const tx = await signAndSendTransactionInstructions(connection, [], wallet, [
    instruction,
  ]);
  return tx;
};

/**
 * Returns the USDC associated token account of a wallet.
 *
 * @param connection The solana connection object to the RPC node.
 * @param wallet The wallet owning the associated USDC account.
 * @returns The public key of the USDC associated token account of the wallet.
 */
export const getQuoteAccount = async (
  wallet: PublicKey
): Promise<PublicKey> => {
  if (!Cache.get("quoteAccount")) {
    const associatedUSDCAccount = await findAssociatedTokenAddress(
      wallet,
      USDC_MINT
    );
    Cache.set("quoteAccount", associatedUSDCAccount);
  }
  // @ts-ignore
  return Cache.get("quoteAccount");
};

/**
 * Returns the FIDA associated token account of a wallet.
 *
 * @param connection The solana connection object to the RPC node.
 * @param wallet The wallet owning the associated USDC account.
 * @returns The public key of the USDC associated token account of the wallet.
 */
export const getDiscountAccount = async (
  connection: Connection,
  wallet: PublicKey,
  env = Env.prod
) => {
  if (env === Env.dev) {
    return undefined;
  }
  if (!Cache.has("discountAccount")) {
    const associatedFidaAccount = await findAssociatedTokenAddress(
      wallet,
      FIDA_MINT
    );
    const accountInfo = await connection.getAccountInfo(associatedFidaAccount);
    if (!accountInfo?.data) return undefined;
    Cache.set("discountAccount", associatedFidaAccount);
  }
  return Cache.get("discountAccount");
};

/**
 * Creates the user account i.e the intermediary account that will hold the collateral.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The address of the market, user accounts are market specific.
 * @param wallet The wallet associated to this user account.
 * @returns The primed transaction that will create the user account.
 */
export const createUserAccount = async (
  connection: Connection,
  marketAddress: PublicKey,
  wallet: PublicKey
): Promise<PrimedTransaction> => {
  const quoteAccount = await getQuoteAccount(wallet);
  const primedTx = await addBudget(
    connection,
    marketAddress,
    0,
    wallet,
    quoteAccount,
    wallet
  );
  return primedTx;
};

/**
 * Deposit collateral (USDC) from a wallet into a user account. This does not affect directly the collateral of your positions.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The address of the market, user accounts are market specific.
 * @param amount The USDC amount to deposit from the wallet into the user account. Need to take into account the decimals i.e 1 USDC = 1 * USDC_DECIMALS = 1_000_000
 * @param wallet The wallet to debit the USDC from.
 * @param userAccount The user account to credit the USDC to.
 * @returns The primed transaction that will deposit collateral into the user account.
 */
export const depositCollateral = async (
  connection: Connection,
  marketAddress: PublicKey,
  amount: number,
  wallet: PublicKey,
  userAccount: PublicKey
): Promise<PrimedTransaction> => {
  // Safety checks
  if (amount <= 0) {
    throw new Error("Invalid amount to withdraw, needs to be > 0");
  }
  const allUserAccounts = await getUserAccountsForOwner(connection, wallet);
  if (!allUserAccounts) {
    throw new Error("No user account found for owner");
  }
  const userAccountExists = allUserAccounts.find((acc) =>
    acc?.market.equals(marketAddress)
  );
  if (!userAccountExists) {
    throw new Error(
      `User account ${userAccount.toBase58()} market is not ${marketAddress.toBase58()}`
    );
  }

  const quoteAccount = await getQuoteAccount(wallet);
  const primedTx = await addBudget(
    connection,
    marketAddress,
    amount,
    wallet,
    quoteAccount,
    wallet,
    userAccount
  );
  return primedTx;
};

/**
 * Withdraw collateral (USDC) from a user account and deposit it into a wallet. This does not affect directly the collateral of your positions.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The address of the market, user accounts are market specific.
 * @param amount The USDC amount to deposit from the user account into the wallet. Need to take into account the decimals i.e 1 USDC = 1 * USDC_DECIMALS = 1_000_000
 * @param wallet The wallet to credit the USDC to.
 * @param userAccount The user account to debit the USDC from.
 * @returns The primed transaction that will deposit collateral into the wallet.
 */
export const withdrawCollateral = async (
  connection: Connection,
  marketAddress: PublicKey,
  amount: number,
  wallet: PublicKey,
  userAccount: PublicKey
): Promise<PrimedTransaction> => {
  // Safety checks
  if (amount <= 0) {
    throw new Error("Invalid amount to withdraw, needs to be > 0");
  }
  const allUserAccounts = await getUserAccountsForOwner(connection, wallet);
  if (!allUserAccounts) {
    throw new Error("No user account found for owner");
  }
  const userAccountExists = allUserAccounts.find((acc) =>
    acc?.market.equals(marketAddress)
  );
  if (!userAccountExists) {
    throw new Error(
      `User account ${userAccount.toBase58()} market is not ${marketAddress.toBase58()}`
    );
  }
  const quoteAccount = await getQuoteAccount(wallet);
  const primedTx = await withdrawBudget(
    connection,
    marketAddress,
    amount,
    quoteAccount,
    wallet,
    userAccount
  );
  return primedTx;
};

/**
 * Opens a position for a user account.
 *
 * @param connection The solana connection object to the RPC node.
 * @param side The side of the position you want to open.
 * @param quoteSize The quote size i.e notional of the position you want to open.
 * @param leverage The leverage of the position you want to open.
 * @param userAccount The user account with which you want to open the position.
 * @param referrerAccount The referrer USDC account of the account (optionnal)
 * @returns The primed transaction that will increase the base size of the position.
 */

export const createPosition = async (
  connection: Connection,
  side: PositionType,
  quoteSize: number,
  leverage: number,
  userAccount: UserAccount,
  referrerAccount: PublicKey | undefined = undefined
): Promise<PrimedTransaction> => {
  const collateral = quoteSize / leverage;
  const discountAccount = await getDiscountAccount(
    connection,
    userAccount.owner
  );
  const primedTx = await openPosition(
    connection,
    side,
    new Numberu64(collateral),
    userAccount.market,
    userAccount.owner,
    leverage,
    userAccount.address,
    discountAccount,
    userAccount.owner,
    referrerAccount,
    BNB_ADDRESS
  );
  return primedTx;
};

/**
 * Increases the base size of a position. This will increase the leverage of your position.
 *
 * @param connection The solana connection object to the RPC node.
 * @param position The position you want to increase the size.
 * @param size The size you want to add to your current position. Need to take into account the decimals.
 * @param wallet The wallet used to trade.
 * @param referrerAccount The referrer USDC account of the account (optionnal)
 * @returns The primed transaction that will increase the base size of the position.
 */
export const increasePositionBaseSize = async (
  connection: Connection,
  position: Position,
  size: number,
  wallet: PublicKey,
  referrerAccount: PublicKey | undefined = undefined,
  predictedEntryPrice: number | undefined = undefined,
  maximumSlippageMargin: number | undefined = undefined
): Promise<PrimedTransaction> => {
  const marketState = await getMarketState(connection, position.marketAddress);
  let sideSign = position.side === "long" ? 1 : -1;
  let targetSize = size + position.vCoinAmount;
  const currentQuoteSize =
    (position.vCoinAmount * marketState.vQuoteAmount) /
    (marketState.vCoinAmount + sideSign * position.vCoinAmount);
  const targetPositionQuoteSize =
    (targetSize * (marketState.vQuoteAmount - sideSign * currentQuoteSize)) /
    (marketState.vCoinAmount - sideSign * (targetSize - position.vCoinAmount));
  const targetLeverage = targetPositionQuoteSize / position.collateral;
  const amountToWithdraw =
    position.collateral - currentQuoteSize / targetLeverage;
  const discountAccount = await getDiscountAccount(connection, wallet);
  const [signersClose, instructionsClose] = await closePosition(
    connection,
    new Numberu64(amountToWithdraw),
    new Numberu64(0),
    position.marketAddress,
    wallet,
    position.positionIndex,
    position.userAccount,
    discountAccount,
    wallet,
    referrerAccount,
    BNB_ADDRESS,
    predictedEntryPrice,
    maximumSlippageMargin
  );
  const [signersIncrease, instructionIncrease] = await increasePosition(
    connection,
    position.marketAddress,
    amountToWithdraw,
    targetLeverage,
    position.positionIndex,
    wallet,
    position.userAccount,
    BNB_ADDRESS,
    discountAccount,
    wallet,
    referrerAccount,
    predictedEntryPrice,
    maximumSlippageMargin
  );
  return [
    [...signersClose, ...signersIncrease],
    [...instructionsClose, ...instructionIncrease],
  ];
};

/**
 * Increases the collateral of a position. This will decrease the leverage of your position.
 *
 * @param connection The solana connection object to the RPC node.
 * @param position The position you want to increase the collateral.
 * @param wallet The wallet used to trade.
 * @param collateral The collateral to add to your current position. Need to take into account the decimals.
 * @returns The primed transaction that will increase the collateral of the position.
 */
export const increasePositionCollateral = async (
  connection: Connection,
  position: Position,
  wallet: PublicKey,
  collateral: number,
  referrerAccount: PublicKey | undefined = undefined
) => {
  const discountAccount = await getDiscountAccount(connection, wallet);
  const primedTx = await increasePosition(
    connection,
    position.marketAddress,
    collateral,
    0,
    position.positionIndex,
    wallet,
    position.userAccount,
    BNB_ADDRESS,
    discountAccount,
    wallet,
    referrerAccount
  );
  return primedTx;
};

/**
 * Reduces the base size of a position. This will decrease the leverage of your position.
 *
 * @param connection The solana connection object to the RPC node.
 * @param position The position you want to reduce the size.
 * @param wallet The wallet used to trade.
 * @param size The base size to deduce from your position. Need to take into account the decimals.
 * @param referrerAccount The referrer USDC account of the account (optionnal)
 * @returns The primed transaction that will decrease the base size of the position.
 */
export const reducePositionBaseSize = async (
  connection: Connection,
  position: Position,
  size: number,
  wallet: PublicKey,
  referrerAccount: PublicKey | undefined = undefined,
  predictedEntryPrice: number | undefined = undefined,
  maximumSlippageMargin: number | undefined = undefined
) => {
  const discountAccount = await getDiscountAccount(connection, wallet);
  const primedTx = await closePosition(
    connection,
    new Numberu64(0),
    new Numberu64(size),
    position.marketAddress,
    wallet,
    position.positionIndex,
    position.userAccount,
    discountAccount,
    wallet,
    referrerAccount,
    BNB_ADDRESS,
    predictedEntryPrice,
    maximumSlippageMargin
  );
  return primedTx;
};

/**
 * Completely close a position.
 *
 * @param connection The solana connection object to the RPC node.
 * @param position The position you want to close.
 * @param wallet The wallet used to trade.
 * @param referrerAccount The referrer USDC account of the account (optionnal)
 * @returns The primed transaction that will close the position.
 */
export const completeClosePosition = async (
  connection: Connection,
  position: Position,
  wallet: PublicKey,
  referrerAccount: PublicKey | undefined = undefined
) => {
  const discountAccount = await getDiscountAccount(connection, wallet);
  const primedTx = await closePosition(
    connection,
    new Numberu64(position.collateral),
    new Numberu64(position.vCoinAmount),
    position.marketAddress,
    wallet,
    position.positionIndex,
    position.userAccount,
    discountAccount,
    wallet,
    referrerAccount,
    BNB_ADDRESS
  );
  return primedTx;
};

/**
 * Reduces the collateral of a position.
 *
 * @param connection The solana connection object to the RPC node.
 * @param position The position you want to reduce the collateral.
 * @param wallet The wallet used to trade.
 * @param collateral The collateral to add to the position.
 * @param referrerAccount The referrer USDC account of the account (optionnal)
 * @returns The primed transaction that will reduce the collateral of the position.
 */
export const reducePositionCollateral = async (
  connection: Connection,
  position: Position,
  wallet: PublicKey,
  collateral: number,
  referrerAccount: PublicKey | undefined = undefined
) => {
  const discountAccount = await getDiscountAccount(connection, wallet);
  const primedTx = await closePosition(
    connection,
    new Numberu64(collateral),
    new Numberu64(0),
    position.marketAddress,
    wallet,
    position.positionIndex,
    position.userAccount,
    discountAccount,
    wallet,
    referrerAccount,
    BNB_ADDRESS
  );
  return primedTx;
};

/**
 * Get all the user positions for a given market.
 *
 * @param connection The solana connection object to the RPC node.
 * @param wallet The wallet to credit the USDC to.
 * @returns An array of Position objects.
 */
export const getOpenPositions = async (
  connection: Connection,
  wallet: PublicKey
) => {
  let positions: Position[] = [];
  const _positions = await getOrders(connection, wallet);
  for (let pos of _positions) {
    const marketState = await getMarketState(connection, pos.market);
    const entryPrice = pos.position.vPcAmount / pos.position.vCoinAmount;
    const leverage = pos.position.vPcAmount / pos.position.collateral;
    const pnl =
      pos.position.side === 1
        ? (pos.position.vCoinAmount * marketState.vQuoteAmount) /
            (marketState.vCoinAmount + pos.position.vCoinAmount) -
          pos.position.vPcAmount
        : pos.position.vPcAmount -
          (pos.position.vCoinAmount * marketState.vQuoteAmount) /
            (marketState.vCoinAmount - pos.position.vCoinAmount);
    const size = pos.position.vCoinAmount;
    const position = {
      side: pos.position.side === 1 ? "long" : "short",
      size: size,
      pnl,
      leverage: leverage,
      liqPrice: pos.position.liquidationIndex,
      entryPrice: entryPrice,
      userAccount: pos.userAccount,
      collateral: pos.position.collateral,
      marketAddress: pos.market,
      positionIndex: pos.position_index,
      vCoinAmount: pos.position.vCoinAmount,
      instanceIndex: pos.position.instanceIndex,
    };
    positions.push(position);
  }
  return positions;
};

/**
 * Get all the information of a trade from its tx.
 *
 * @param connection The solana connection object to the RPC node.
 * @param tx The tx of the trade
 * @returns A PastTrade object.
 */
export const getTradeInfoFromTx = async (
  connection: Connection,
  tx: string
) => {
  const tradeInfo = await extractTradeInfoFromTransaction(connection, tx);
  return tradeInfo;
};

/**
 * Get recent market trades.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The address of the market.
 * @param limit The limit to the number of trades to return (optional)
 * @returns An array of PastTrade objects.
 */
export const getMarketTrades = async (
  connection: Connection,
  marketAddress: PublicKey,
  limit = 100
) => {
  const pastTrades = await getPastTrades(connection, marketAddress, {
    limit: limit,
  });
  return pastTrades;
};

/**
 * Get current mark price.
 *
 * @param connection The solana connection object to the RPC node.
 * @param marketAddress The address of the market.
 * @returns The current mark price of the vAMM.
 */
export const getMarkPrice = async (
  connection: Connection,
  marketAddress: PublicKey
) => {
  const markPrice = await (
    await getMarketState(connection, marketAddress)
  ).getMarkPrice();
  return markPrice;
};
