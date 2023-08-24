import * as ContextMenu from "@radix-ui/react-context-menu";
import {CollectionDefinitionExtended, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {FC} from "react";
import {deleteItem, replaceItem} from "../../utils/CollectionReplaceUtils";
import {CollectionDefinition, ItemDefinition} from "postman-collection";


type ItemContextMenuProps = {
    item: ItemDefinition,
    collection: CollectionDefinition
}

export const ItemContextMenu:FC<ItemContextMenuProps> = ({item, collection})=>{
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
        <ContextMenu.Item className=" p-1 cursor-pointer">Open in Tab</ContextMenu.Item>
        <ContextMenu.Item className=" p-1 cursor-pointer">Rename</ContextMenu.Item>
        <ContextMenu.Item className=" p-1 cursor-pointer">Duplicate</ContextMenu.Item>
        <ContextMenu.Item className=" p-1 cursor-pointer" onClick={()=>{deleteSelectedItem()}}>Delete</ContextMenu.Item>
    </>
}
