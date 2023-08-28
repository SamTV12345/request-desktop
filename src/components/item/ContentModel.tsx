import {ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {QueryParam} from "./QueryParam";
import ResizableBox from "../resizable/ResizableBox";
import {ResponseBar} from "./responseBarItems/ResponseBar";
import {ItemRequestBar} from "./ItemRequestBar";
import {ItemNameAdapter} from "./ItemNameAdapter";

export const ContentModel = () => {
    const currentItem = useAPIStore(state => state.currentItem) as ItemDefinitionExtended
    const responseExtended = useAPIStore(state => state.responseExtended)

    return (
        <div className="request-view">
            {currentItem && currentItem.request && <>
                <ItemNameAdapter/>
                <ItemRequestBar/>
                <QueryParam/>
                {
                    <ResizableBox direction={"top"} initialSize={50} className={`response-section border-t-2 border-gray-500 ${!responseExtended&& '!h-10'}`}>
                        <ResponseBar/>
                    </ResizableBox>
                }
            </>}
        </div>
    )
}
