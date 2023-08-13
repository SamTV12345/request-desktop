import {AuthorizationTypes} from "../../models/AuthorizationTypes";
import {AuthorizationSelector} from "../authorization/AuthorizationSelector";
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {AuthorizationContent} from "../authorization/AuthorizationContent";

export const CollectionAuthorization = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const updateAuthorization = (authChange: AuthorizationTypes)=>{
        const clonedCollection:CollectionDefinitionExtended = {
            ...currentCollection,
            auth: {
                ...currentCollection?.auth,
                type: authChange
            },
            type: DisplayType.COLLECTION_TYPE
        }
        updateCollection(clonedCollection!)
    }


    return <div className="">
        This authorization method will be used for every request in this collection. You can override this by specifying one in the request.
        <div className="flex p-2 gap-10">
            <AuthorizationSelector onChange={(v)=>updateAuthorization(v)} value={currentCollection?.auth?.type as AuthorizationTypes}/>
            <div className="border-2 border-gray-700"></div>
            <AuthorizationContent/>
        </div>
    </div>
}
