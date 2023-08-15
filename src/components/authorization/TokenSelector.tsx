import * as Select from "@radix-ui/react-dropdown-menu";
import {useAPIStore} from "../../store/store";
import {useEffect, useMemo, useState} from "react";
import {TokenWithKey} from "../../models/OAuth2Outcome";
import {parseJWT} from "./TokenManager";
import {getAllTokens, getTokenByCollectionId, setTokenToItem} from "./TokenManagerService";

export const TokenSelector = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const setOpenManager = useAPIStore(state=>state.setOpenTokenManager)
    const tokens = useAPIStore(state=>state.tokens)
    const setTokens = useAPIStore(state=>state.setTokens)
    const [selectedToken, setSelectedToken] = useState<TokenWithKey|undefined>(undefined)
    let db

    useEffect(() => {
        getAllTokens().then((tokens) => {
            setTokens(tokens)
        })
    }, [])

    const changeSelectedToken = (token:TokenWithKey)=>{
        setSelectedToken(token)
        setTokenToItem(token.key, currentCollection!.info._postman_id)
    }

    useEffect(() => {
        getTokenByCollectionId(currentCollection?.info._postman_id!).then((token) => {
          setSelectedToken(token)
        })
            .catch(e=>{
                console.log(e)
                setSelectedToken(undefined)
            })
    }, []);

    return   <Select.Root>
        <Select.Trigger placeholder="Available Tokens" className={`flex items-center pl-6 pr-2 py-2 text-sm bg-basecol w-full text-white`}>

            {selectedToken ? selectedToken.token_name: "Select Token"}
        </Select.Trigger>

            <Select.Content className="overflow-hidden rounded-lg shadow-[0_4px_16px_rgba(0,0,0,0.2)] z-30 bg-basecol w-[600px]">


                    {tokens.map((token) =>
                        <Select.Item key={token.key} onClick={()=>changeSelectedToken(token)} className="relative pl-6 pr-4 py-1.5 rounded text-white text-sm hover:bg-mustard-600 hover:text-white">
                            <Select.ItemIndicator className="absolute left-0" onClick={()=>{

                            }}>
                                <span className="material-symbols-outlined align-middle !leading-none !text-xl">check</span>
                            </Select.ItemIndicator>

                            <Select.Label>{token.token_name}</Select.Label>
                        </Select.Item>)}
                <Select.Separator className="border-[1px] border-white"></Select.Separator>
                        <Select.Item onClick={()=>{setOpenManager(true)}}
                                     className="relative pl-6 pr-4 py-1.5 rounded text-white text-sm hover:bg-mustard-600 hover:text-white">
                            Manage tokens</Select.Item>
            </Select.Content>
    </Select.Root>
}
