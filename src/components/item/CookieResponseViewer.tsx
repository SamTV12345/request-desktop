import {useAPIStore} from "../../store/store";
import {ResultTable} from "./ResultTable";

export const CookieResponseViewer = ()=>{
    const currentRequest = useAPIStore(state => state.currentRequest)

    return <ResultTable value={currentRequest?.cookies!} emptyMessage={"No cookies received from server"}/>
}
