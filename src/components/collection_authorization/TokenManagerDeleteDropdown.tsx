import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import {deleteToken, getAllTokens} from "./TokenManagerService";
import {useAPIStore} from "../../store/store";
import {isTokenExpired} from "../../utils/utils";

export const TokenManagerDeleteDropdown = ()=>{
    const setTokens = useAPIStore(state=>state.setTokens)

    return <DropdownMenu.Root>
        <DropdownMenu.Trigger className="flex items-center text-sm bg-basecol w-full text-red-400">
            Delete
            <span className="material-symbols-outlined">expand_more</span>
        </DropdownMenu.Trigger>
        <DropdownMenu.Content className="overflow-hidden rounded-lg shadow-[0_4px_16px_rgba(0,0,0,0.2)]
         z-30 bg-basecol">
            <DropdownMenu.Item className="relative pl-6 pr-4 py-1.5 rounded text-sm text-stone-500 hover:bg-stone-500" >
                <DropdownMenu.ItemIndicator className="absolute left-0">
                    <span className="material-symbols-outlined align-middle !leading-none !text-xl">check</span>
                </DropdownMenu.ItemIndicator>
                <DropdownMenu.Label onClick={()=>{
                    getAllTokens().then((tokens) => {
                        const deletedTokensKeys:string[] = []
                        for (const token of tokens) {
                            if (isTokenExpired(token)) {
                                deleteToken(token.key)
                                deletedTokensKeys.push(token.key)
                            }
                        }
                        setTokens(tokens.filter(token => !deletedTokensKeys.includes(token.key)))
                    })
                }} className="cursor-pointer">Expired tokens</DropdownMenu.Label>
            </DropdownMenu.Item>
            <DropdownMenu.Item>
                <DropdownMenu.Label onClick={async () => {
                    const res = await getAllTokens()
                    for (const token of res) {
                        deleteToken(token.key)
                        setTokens([])
                }
                }} className="relative pl-6 pr-4 py-1.5 rounded text-sm hover:text-white hover:bg-stone-500 cursor-pointer">
                    All tokens</DropdownMenu.Label>
            </DropdownMenu.Item>
        </DropdownMenu.Content>
    </DropdownMenu.Root>
}
