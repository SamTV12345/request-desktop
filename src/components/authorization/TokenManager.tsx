import * as Dialog from "@radix-ui/react-dialog";
import {useEffect, useState} from "react";
import {Token, TokenLoadResult, TokenWithKey} from "../../models/OAuth2Outcome";
import {useAPIStore} from "../../store/store";
import {TokenManagerDeleteDropdown} from "./TokenManagerDeleteDropdown";
import {getAllTokens} from "./TokenManagerService";


export const parseJWT = (token: TokenLoadResult): Token => {
    const base64Url = token.access_token.split('.')[1];
    const base64 = base64Url.replace(/-/g, '+').replace(/_/g, '/');
    const jsonPayload = decodeURIComponent(window.atob(base64).split('').map(function(c) {
        return '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2);
    }).join(''));

    return {
        ...JSON.parse(jsonPayload),
        access_token: token.access_token,
        token_name: token.token_name
    };
}

export const TokenManager = ()=>{
    const [tokens, setTokens] = useState<TokenWithKey[]>([])
    let db:IDBDatabase
    const openTokenManager = useAPIStore(state=>state.openTokenManager)
    const setTokenManger = useAPIStore(state=>state.setOpenTokenManager)
    const [selectedToken, setSelectedToken] = useState<TokenWithKey|undefined>(undefined)

    useEffect(() => {
        const token:TokenWithKey[] = []
        const dbRq = indexedDB.open("tokens", 1)
        dbRq.onupgradeneeded = (event)=>{
            db = (event.target as any).result
            const tokenStore = db.createObjectStore("tokens", {
                autoIncrement: true
            })
            db.createObjectStore("item-to-token", {
                autoIncrement:true
            })
        }
        getAllTokens().then((tokens) => {
            setTokens(tokens)
        })
    }, []);

    const showValues = (storeName:unknown)=>{
        if(typeof storeName === "string"){
            return storeName
        }
        else if (typeof storeName === "number") {
            return new Date(storeName * 1000).toString()
        }
        else if (typeof storeName === "object"){
            return JSON.stringify(storeName)
            }
        }

    return <Dialog.Root open={openTokenManager}>
        <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 bg-black opacity-30"/>
            <Dialog.Content className="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 bg-basecol rounded-lg p-6">
                <Dialog.Title className="text-white font-bold text-2xl uppercase">Manage Access Tokens</Dialog.Title>
                <div className="flex gap-5">
                    <div>
                        <div className="text-white grid grid-cols-2 gap-5"><span>All tokens</span> <TokenManagerDeleteDropdown/></div>
                        <div className="flex flex-col gap-0">
                    {
                        tokens.map((token)=> {
                            return <button key={token.key} onClick={()=>{
                                setSelectedToken(token)
                            }} className="text-white text-left">{token.token_name}</button>
                        })
                        }
                        </div>
                    </div>
                    <div className="border-[1px] border-white"></div>
                    <div>
                        {
                            selectedToken?Object.entries(selectedToken).map(([key, value])=>{
                                return <div className="grid grid-cols-2 gap-3 text-white">
                                    <div>{key}</div>
                                    <div>{showValues(value)}</div>
                                </div>
                            }):<span className="text-white">No tokens selected</span>
                        }
                    </div>
                </div>
                <Dialog.Close  className="absolute top-0 right-0 p-2 text-white">
                    <button className="IconButton" aria-label="Close">
                        <span className="material-symbols-outlined" onClick={()=>setTokenManger(false)}>Close</span>
                    </button>
                </Dialog.Close>
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>
}
