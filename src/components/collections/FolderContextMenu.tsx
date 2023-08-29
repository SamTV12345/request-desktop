import {FC} from "react";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import * as ContextMenu from "@radix-ui/react-context-menu";
import {ItemGroupDefinition} from "postman-collection";
import {addNewItem, deleteItem} from "../../utils/CollectionReplaceUtils";
import {getDefaultFolder, getDefaultRequest} from "../../constants/config";

type CollectionContextMenuProps = {
    item: ItemGroupDefinition,
    collection: CollectionDefinitionExtended
}

export const FolderContextMenu:FC<CollectionContextMenuProps> = ({collection, item})=>{
    const updateCollection = useAPIStore(state=>state.saveGivenCollection)
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const saveCollection = useAPIStore(state => state.saveGivenCollection)

    const deleteSelectedItem = ()=>{
        const mappedCollection = deleteItem(collection,item.id!) as CollectionDefinitionExtended
        updateCollection(mappedCollection)
        if(currentCollection?.id === collection.id){
            useAPIStore.getState().setCurrentCollection(mappedCollection)
        }
    }

    const createItem = (collection:CollectionDefinitionExtended, parentId: string)=>{
        const itemToInsert = getDefaultRequest()
        const collectionChanged = addNewItem(collection, parentId, itemToInsert)
        saveCollection(collectionChanged as CollectionDefinitionExtended)
    }

    const createFolder = (collection:CollectionDefinitionExtended, parentId: string)=>{
        console.log("create folder", collection, parentId)
        const itemToInsert = getDefaultFolder()
        const collectionChanged = addNewItem(collection, parentId, itemToInsert)
        saveCollection(collectionChanged as CollectionDefinitionExtended)
    }

    return <>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={()=>createItem(collection,item.id!)}>Add Request</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={()=>createFolder(collection,item.id!)}>Add Folder</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Rename</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Duplicate</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={deleteSelectedItem}>Delete</ContextMenu.Item>
    </>
}
