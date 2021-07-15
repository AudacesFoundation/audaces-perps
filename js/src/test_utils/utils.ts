import {Account, Connection, PublicKey} from '@solana/web3.js'

import {AccountInfo, Token, TOKEN_PROGRAM_ID} from '@solana/spl-token'

export class TokenMint {

    constructor(public token: Token, public signer: Account){ }

    static async init(connection: Connection, feePayer: Account){
        let signer = new Account();
        let token = await Token.createMint(connection, feePayer, signer.publicKey, null, 6, TOKEN_PROGRAM_ID);
        return new TokenMint(token, signer)
    }

    async getAssociatedTokenAccount(wallet: PublicKey): Promise<PublicKey> {
        let acc = await this.token.getOrCreateAssociatedAccountInfo(wallet);
        return acc.address
    }

    async mintInto(tokenAccount: PublicKey, amount: number): Promise<void> {
        return this.token.mintTo(tokenAccount, this.signer, [], amount)
    }
}

export async function sleep(ms: number){
    console.log("Sleeping for ", ms, " ms");
    return await new Promise(resolve => setTimeout(resolve, ms))
}