# JS Bindings Documentation

A JavaScript client library for interacting with the on-chain program. This library can be used for:

- Creating markets
- Trading on a market
- Fetching market data
- Cranking (Rust sample code is recommended ⚠️)

## Installation

Using npm

```
npm i @audaces/perps
```

Using yarn

```
yarn add @audaces/perps
```

## Concepts

- **User Account:** The user account is the intermediary account between the trader's wallet and the on-chain market. This is where the collateral of the trader is held and funding is extracted. Note that user accounts are market specific.

- **Managing position:** Positions are isolated, this means that longs and shorts do not offset each other (e.g opening a long of size `x` and a short of size `x` results in having both a long and a short of size `x`). Open positions need to be modified with the appropriate bindings (see sections below)

## High level bindings

The high level bindings (`high_level.ts`) contain higher level bindings that aim at providing an easier way to interact with the on-chain program than the low level bindings (`bindings.ts` and `secondary_bindings.ts`).

### Managing a position

#### Creating a user account

Creating a user account for a particular market (`marketAddress`) can be done with `createUserAccount`

```js
const [signers, instructions] = await createUserAccount(
  connection,
  marketAddress,
  wallet
);

await signAndSendTransactionInstructions(
  connection,
  signers,
  feePayer,
  instructions
);
```

To deposit/withdraw collateral into a user account

```js
// Deposit
const [signers, instructions] = await depositCollateral(
  connection,
  marketAddress,
  amount, // Amount of collateral to deposit /!\ With decimals
  wallet, // Wallet depositing collateral into the user account
  userAccount // Address of the user account
);

await signAndSendTransactionInstructions(
  connection,
  signers,
  feePayer,
  instructions
);

// Withdraw
const [signers, instructions] = await withdrawCollateral(
  connection,
  marketAddress,
  amount, // Amount of collateral to withdraw /!\ With decimals
  wallet, // Wallet getting the collateral back
  userAccount // Address of the user account
);

await signAndSendTransactionInstructions(
  connection,
  signers,
  feePayer,
  instructions
);
```

#### Opening a position

Opening a position can be done with `createPosition`

```js
const [signers, instructions] = await createPosition(
  connection,
  side,
  quoteSize,
  leverage,
  userAccount
);

await signAndSendTransactionInstructions(
  connection,
  signers,
  feePayer,
  instructions
);
```

<br/>

#### Editing position size

Sizes can be edited with `increasePositionBaseSize`, `reducePositionBaseSize` and `completeClosePosition`. The following snippet increases the base size of a `position` by `baseSize`

```js
const [signers, instructions] = await increasePositionBaseSize(
  connection,
  position,
  baseSize,
  wallet
);

await signAndSendTransactionInstructions(
  connection,
  signers,
  feePayer,
  instructions
);
```

⚠️ These transactions will change the leverage of the position.

<br/>

#### Editing position collateral

Collateral can be changed with `increasePositionCollateral` and `reducePositionCollateral`. The following snippet increases the collateral of a `position` by `collateral`

```js
const [signers, instructions] = await increasePositionCollateral(
  connection,
  position,
  wallet,
  collateral
);

await signAndSendTransactionInstructions(
  connection,
  signers,
  feePayer,
  instructions
);
```

⚠️ These transactions will change the leverage of the position.

<br/>

### Getting open positions

Getting all the open positions of a trader can be done using `getOpenPositions`

```js
const openPositions = await getOpenPositions(connection, wallet);
```

This function returns an array of Position objects (i.e `Position[]`). The `Position` interface is defined as follow:

```js
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
}
```

<br/>

### Getting trade information

Getting all the details of a trade after it got executed form the transaction's signature can be done using `getTradeInfoFromTx`.

```js
const tradeInfo = await getTradeInfoFromTx(connection, tx);
```

<br/>

### Fetching market data

Market trades can be fetched from the blockchain directly. Note that fetching lots of transactions is slow and expensive. By default the limit parameter of this function is 100.

```js
// With default limit
const marketTrades = await getMarketTrades(connection, marketAddress);

// With limit = 20
const marketTrades20 = await getMarketTrades(connection, marketAddress, 20);
```

<br/>

To fetch the mark price one can use `getMarkPrice`

```js
const markPrice = await getMarkPrice(connection, marketAddress);
```

<br/>

To fetch the oracle price and confidence interval (using Pyth Network oracle)

```js
const { price, confidence } = await getOraclePrice(
  connection,
  oraclePriceAccountAddress
);
```
