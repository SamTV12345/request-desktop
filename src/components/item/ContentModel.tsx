import {useAPIStore} from "../../store/store";
import {QueryParam} from "./QueryParam";
import ResizableBox from "../resizable/ResizableBox";
import {ResponseBar} from "./responseBarItems/ResponseBar";
import {ItemRequestBar} from "./ItemRequestBar";
import {ItemNameAdapter} from "./ItemNameAdapter";


export const ContentModel = () => {
    const currentItem = useAPIStore(state => state.currentItem)
    const currentRequest = useAPIStore(state => state.currentRequest)


    return (
        <div className="request-view">
            {currentItem && currentItem.request && <>
                <ItemNameAdapter/>
                <ItemRequestBar/>
                <QueryParam/>
                {currentRequest &&
                    <ResizableBox direction={"top"} initialSize={300} className="response-section border-t-2 border-gray-500">
                        <ResponseBar/>
                    </ResizableBox>}
            </>}
        </div>
    )
}
