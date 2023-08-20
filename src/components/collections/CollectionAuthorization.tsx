import {AuthorizationTypes} from "../../models/AuthorizationTypes";
import {AuthorizationSelector} from "../collection_authorization/AuthorizationSelector";
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {AuthorizationContent} from "../collection_authorization/AuthorizationContent";
import {isItemsGroupDefinition} from "../../utils/utils";
import {ItemGroupDefinition} from "postman-collection";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {useMemo} from "react";

export const CollectionAuthorization = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const currentItem = useAPIStore(state=>state.currentItem)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const updateCurrentItem = useAPIStore(state=>state.setCurrentItem)

    const authType = useMemo(
        ()=>{
            if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
                return currentCollection?.auth?.type
            }
            else if (isItemsGroupDefinition(currentItem)) {
                return currentItem?.auth?.type
            }
            else if (currentItem!==undefined) {
                return currentItem?.request?.auth?.type
            }
            else if (authType === undefined)
                return undefined
        }
    ,[currentItem,currentCollection])
    const updateAuthorization = (authChange: AuthorizationTypes)=>{
        if(currentCollection?.type === DisplayType.COLLECTION_TYPE) {
            const clonedCollection: any = {
                ...currentCollection,
                auth: {
                    ...currentCollection?.auth,
                    type: authChange
                },
                type: DisplayType.COLLECTION_TYPE
            }
            updateCollection(clonedCollection!)
        }
        else if (currentItem!==undefined) {
            if (isItemsGroupDefinition(currentItem)) {

                const clonedItem:ItemGroupDefinition = {
                    ...currentItem,
                    auth:{
                        ...currentItem?.auth,
                        type: authChange
                    }
                }
                const replacedCollection = replaceItem(currentCollection as CollectionDefinitionExtended, clonedItem) as CollectionDefinitionExtended
                updateCollection(replacedCollection!)
                updateCurrentItem(clonedItem)
                saveCollection()
            } else {
                const clonedItem = {
                    ...currentItem,
                    request: {
                        ...currentItem?.request,
                        auth: {
                            ...currentItem?.request?.auth,
                            type: authChange
                        }
                    }
                }
                const replacedCollection = replaceItem(currentCollection as CollectionDefinitionExtended, clonedItem) as CollectionDefinitionExtended
                updateCollection(replacedCollection!)
                updateCurrentItem(clonedItem)
                saveCollection()
            }
        }
    }


    return <div className="">
        This authorization method will be used for every request in this collection. You can override this by specifying one in the request.
        <div className="flex p-2 gap-10">
            <AuthorizationSelector onChange={(v)=>updateAuthorization(v)} value={authType as AuthorizationTypes}/>
            <div className="border-2 border-gray-700"></div>
            <AuthorizationContent/>
        </div>
    </div>
}
