import * as Dialog from "@radix-ui/react-dialog";
import {useEffect, useMemo, useState} from "react";
import {useAPIStore} from "../../store/store";
import {OAuth2FailureOutcome, OAuth2SucessOutcome} from "../../models/OAuth2Outcome";
import {insertToken} from "./TokenManagerService";

const isOAuth2Success = (oauth2Object: OAuth2SucessOutcome|OAuth2FailureOutcome|undefined): oauth2Object is OAuth2SucessOutcome=>{
    return oauth2Object?oauth2Object.hasOwnProperty("access_token"):false
}

const isOAuth2Failure = (oauth2Object: OAuth2SucessOutcome|OAuth2FailureOutcome|undefined):oauth2Object is OAuth2FailureOutcome  =>{
    return oauth2Object? oauth2Object.hasOwnProperty("message"): false
}
export const OAuth2Loader = () => {
    const [countDown, setCountDown] = useState<number>(5)
    const openSuccessOAuth2 = useAPIStore(state=>state.openOAuth2Screen)
    const setOpenSuccessOAuth2 = useAPIStore(state=>state.setOAuth2Screen)
    const oauth2Playload = useAPIStore(state=>state.oauth2Outcome)
    const collection = useAPIStore(state=>state.currentCollection)

    useEffect(() => {
        if(openSuccessOAuth2 && isOAuth2Success(oauth2Playload)){
            setCountDown(5)
        }
    }, [openSuccessOAuth2]);

    useEffect(() => {
        countDown > 0 && openSuccessOAuth2  && setTimeout(() => setCountDown(countDown - 1), 1000);
    }, [countDown, openSuccessOAuth2]);

    useEffect(() => {

        if(countDown === 0 && isOAuth2Success(oauth2Playload)){
            // @ts-ignore
            insertToken( crypto.randomUUID(),oauth2Playload.access_token, oauth2Playload.token_name,collection?.info._postman_id as string )
            setOpenSuccessOAuth2(false)
        }
    }, [countDown]);


    return <Dialog.Root open={openSuccessOAuth2}>
        <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 bg-black opacity-30"/>
            <Dialog.Content className="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 bg-basecol rounded-lg p-6">
                <Dialog.Title className="text-white font-bold text-2xl">Get new access token</Dialog.Title>
                {isOAuth2Success(oauth2Playload)? <div>
                    <div className="flex justify-center mt-5">
                        <span className="material-symbols-outlined rounded-full bg-green-700 text-white text-center">check</span>
                    </div>

                        <Dialog.Description className="text-white mt-2">You will be redirected to the token manager in {countDown}</Dialog.Description>
                </div>
                    :<div>

                        <div>
                            <Dialog.Description className="text-white mt-2">
                                {isOAuth2Failure(oauth2Playload)?
                                <>
                                    <div className="flex justify-center mt-5">
                                        <span
                                            className="material-symbols-outlined rounded-full bg-red-700 text-white text-center">Close</span>
                                    </div>
                                    <Dialog.Description className="text-white mt-2 font-bold text-center">Authentication failed</Dialog.Description>
                                <Dialog.Description className="text-white mt-2">{oauth2Playload.description} </Dialog.Description></>
                                :<div></div>}</Dialog.Description>
                        </div>
                    </div>

                }
                <Dialog.Close  className="absolute top-0 right-0 p-2 text-white">
                    <button className="IconButton" aria-label="Close">
                        <span className="material-symbols-outlined" onClick={()=>setOpenSuccessOAuth2(false)}>Close</span>
                    </button>
                </Dialog.Close>
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>
}
