import {
    Account,
    PublicKey,
    SystemProgram,
    TransactionInstruction,
    Connection,
    TokenAmount,
    ConfirmedSignatureInfo,
    CompiledInnerInstruction,
    CompiledInstruction,
    CreateAccountParams,
    ConfirmedTransaction,
  } from '@solana/web3.js';
import BN from 'bn.js';
import assert from 'assert';


export async function changeOraclePrice(
    connection: Connection,
    oracleProgramId: PublicKey,
    newPrice: number,
    payer: PublicKey,
    oracleKey?: PublicKey,
  ): Promise<[Account[], TransactionInstruction[]]> {
    
    let instructions:TransactionInstruction[] = [];
    let signers:Account[] = [];
    if (!oracleKey){
      let oracle = new Account();
      oracleKey = oracle.publicKey;
      let p: CreateAccountParams = {
        fromPubkey: payer,
        newAccountPubkey: oracleKey,
        lamports: await connection.getMinimumBalanceForRentExemption(8),
        space: 8,
        programId: oracleProgramId,
      };
      instructions.push(SystemProgram.createAccount(p));
      signers.push(oracle);
    }

    //Overwrite the data
    let buffers = [
        Buffer.from(Uint8Array.from([0])),
        //@ts-ignore
        new Numberu64(newPrice).toBuffer(),
    ];
    
    const data = Buffer.concat(buffers);
    const keys = [
        {
        pubkey: oracleKey,
        isSigner: false,
        isWritable: true,
        }
    ];
    instructions.push(new TransactionInstruction({
        keys,
        programId: oracleProgramId,
        data,
    }));

    return [signers, instructions]
}

export class Numberu64 extends BN {
    /**
     * Convert to Buffer representation
     */
    toBuffer(): Buffer {
      const a = super.toArray().reverse();
      const b = Buffer.from(a);
      if (b.length === 8) {
        return b;
      }
      assert(b.length < 8, 'Numberu64 too large');
  
      const zeroPad = Buffer.alloc(8);
      b.copy(zeroPad);
      return zeroPad;
    }
  
    /**
     * Construct a Numberu64 from Buffer representation
     */
    static fromBuffer(buffer): any {
      assert(buffer.length === 8, `Invalid buffer length: ${buffer.length}`);
      return new BN(
        [...buffer]
          .reverse()
          .map(i => `00${i.toString(16)}`.slice(-2))
          .join(''),
        16,
      );
    }
}