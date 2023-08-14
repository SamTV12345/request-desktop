import * as Dialog from "@radix-ui/react-dialog";
import {useEffect, useState} from "react";
import {Token, TokenLoadResult, TokenWithKey} from "../../models/OAuth2Outcome";


const parseJWT = (token: TokenLoadResult): Token => {
    const base64Url = token.access_token.split('.')[1];
    const base64 = base64Url.replace(/-/g, '+').replace(/_/g, '/');
    const jsonPayload = decodeURIComponent(window.atob(base64).split('').map(function(c) {
        return '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2);
    }).join(''));

    return {
        ...JSON.parse(jsonPayload),
        token_name: token.token_name
    };
}

export const TokenManager = ()=>{
    const [tokens, setTokens] = useState<TokenWithKey[]>([])
    let db:IDBDatabase
    console.log(tokens)

    useEffect(() => {
        const dbRq = indexedDB.open("tokens", 1)
        dbRq.onsuccess = (event)=>{
            db = (event.target as any).result
            const tokenStore = db.transaction("tokens", "readwrite").objectStore("tokens")
            tokenStore.openCursor().onsuccess = (event:any)=>{
                const cursor = event.target.result
                if(cursor){
                    const val = parseJWT(cursor.value)
                    setTokens((prev)=>[...prev, {key: cursor.key, ...val}])
                    cursor.continue()
                }
            }
        }
        dbRq.onupgradeneeded = (event)=>{
            db = (event.target as any).result
            const tokenStore = db.createObjectStore("tokens", {
                autoIncrement: true
            })
        }
    }, []);


    return <Dialog.Root open={false}>
        <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 bg-black opacity-30"/>
            <Dialog.Content className="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 bg-basecol rounded-lg p-6">
                <Dialog.Title className="text-white font-bold text-2xl uppercase">Manage Access Tokens</Dialog.Title>
                <div>
                    {
                        tokens.map((token)=> {
                            return <div>{token.token_name}</div>
                        })
                        }
                </div>
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>
}
