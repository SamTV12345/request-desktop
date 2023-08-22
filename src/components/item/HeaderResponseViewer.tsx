import {ResultTable} from "./ResultTable";
import {useAPIStore} from "../../store/store";


export const HeaderResponseViewer = () => {
    const currentRequest = useAPIStore(state => state.currentRequest)

    return <ResultTable value={currentRequest?.headers!} emptyMessage={"No headers received from server"}/>
}
