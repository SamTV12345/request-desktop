import {FC} from "react";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import * as ContextMenu from "@radix-ui/react-context-menu";
import {useNavigate} from "react-router-dom";
import {getDefaultFolder, getDefaultRequest} from "../../constants/config";
import {addNewItem} from "../../utils/CollectionReplaceUtils";

type CollectionContextMenuProps = {
    collection: CollectionDefinitionExtended
}

export const CollectionContextMenu:FC<CollectionContextMenuProps> = ({collection})=>{
    const deleteCollectionFromApp = useAPIStore(state=>state.deleteCollection)
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const navigate = useNavigate()
    const saveCollection = useAPIStore(state => state.saveGivenCollection)
    const deleteCollection = ()=>{
        if (collection.info._postman_id === currentCollection?.info._postman_id){
            navigate("/")
        }
        deleteCollectionFromApp(collection.info._postman_id)
    }


    const createItem = (collection:CollectionDefinitionExtended, parentId: string)=>{
        const itemToInsert = getDefaultRequest()
        const collectionChanged = addNewItem(collection, parentId, itemToInsert)
        saveCollection(collectionChanged as CollectionDefinitionExtended)
    }

    const createFolder = (collection:CollectionDefinitionExtended, parentId: string)=>{
        const itemToInsert = getDefaultFolder()
        const collectionChanged = addNewItem(collection, parentId, itemToInsert)
        saveCollection(collectionChanged as CollectionDefinitionExtended)
    }


    return <>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={()=>createItem(collection,collection.info._postman_id)}>Add Request</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={()=>createFolder(collection, collection.info._postman_id)}>Add Folder</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Rename</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Duplicate</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Export</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={()=>deleteCollection()}>Delete</ContextMenu.Item>
    </>
}
