import {FC} from "react";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import * as ContextMenu from "@radix-ui/react-context-menu";
import {ItemGroupDefinition} from "postman-collection";
import {deleteItem} from "../../utils/CollectionReplaceUtils";

type CollectionContextMenuProps = {
    item: ItemGroupDefinition,
    collection: CollectionDefinitionExtended
}

export const FolderContextMenu:FC<CollectionContextMenuProps> = ({collection, item})=>{
    const updateCollection = useAPIStore(state=>state.saveGivenCollection)
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const deleteSelectedItem = ()=>{
        const mappedCollection = deleteItem(collection,item.id!) as CollectionDefinitionExtended
        updateCollection(mappedCollection)
        if(currentCollection?.id === collection.id){
            useAPIStore.getState().setCurrentCollection(mappedCollection)
        }
    }


    return <>
        <ContextMenu.Item className="p-1 cursor-pointer">Add Request</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Add Folder</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Rename</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Duplicate</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer" onClick={deleteSelectedItem}>Delete</ContextMenu.Item>
    </>
}
