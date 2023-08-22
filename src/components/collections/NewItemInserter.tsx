import * as Dialog from '@radix-ui/react-dialog'
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {useState} from "react";
import {Collection, CollectionDefinition, ItemDefinition} from "postman-collection";
import {getDefaultRequest} from "../../constants/config";
import {addNewItem, replaceItem} from "../../utils/CollectionReplaceUtils";
export const NewItemInserter = () => {
    const newItemInserterOpen = useAPIStore(state => state.newItemInserterOpen)
    const setNewItemInserterOpen = useAPIStore(state => state.setNewItemInserterOpen)
    const currentCollections = useAPIStore(state => state.collections)
    const [currentItem,setCurrentItem] = useState<CollectionDefinition>()
    const [parents, setParents] = useState<any[]>([])
    const saveCollection = useAPIStore(state => state.saveGivenCollection)
    const isItemGroupDefinition = (item: any): item is ItemDefinition => {
        return typeof item === "object" && 'item' in item && item.item != undefined
    }

    const isCollectionDefinition = (item: any): item is CollectionDefinition => {
        return typeof item === "object" && 'info' in item && item.item == undefined
    }
    const changeToSubDirectory = (collection: Collection|ItemDefinition)=>{
        setCurrentItem(collection)
        !isCollectionDefinition(collection)&& setParents([...parents,collection])
    }

    const CollectionItem = ({collection}:{collection: CollectionDefinition})=>{
        const isItemGroup = isItemGroupDefinition(collection)

        return <div className="bg-mustard-600 flex gap-5 p-2 border-[1px] border-black cursor-pointer" onClick={()=>{
            isItemGroup&& changeToSubDirectory(collection)
        }}>
            {isItemGroup?
                <span className="material-symbols-outlined self-center">filter_none</span>
                :<span className="material-symbols-outlined self-center">sync_alt</span>
            }
            <span>{collection.info?collection.info?.name:collection.name}</span>
            <span className="flex-grow"></span>
            {isItemGroup&&<span className="material-symbols-outlined">chevron_right</span>}
        </div>
    }

    const getIdOfItem = (collection: CollectionDefinition)=>{
        // @ts-ignore
        return collection.id || collection.info!._postman_id
    }
    const goBackToElement = (collection: CollectionDefinition)=>{
        let idOfItem = getIdOfItem(collection)
        // @ts-ignore
        const newParent = [...parents]

        while (getIdOfItem(newParent[newParent.length-1]) !== idOfItem){
            newParent.pop()
        }
        setParents(newParent)
        setCurrentItem(collection)
    }

    const goToBack = ()=>{
        setParents([])
        setCurrentItem(undefined)
    }


    const createItem = (collection:CollectionDefinitionExtended, parentId: string)=>{
        const itemToInsert = getDefaultRequest()
        const collectionChanged = addNewItem(collection, parentId, itemToInsert)
        saveCollection(collectionChanged as CollectionDefinitionExtended)
        setCurrentItem({...currentItem, item: [...currentItem?.item!, itemToInsert]})
    }

    return <Dialog.Root open={newItemInserterOpen}>
        <Dialog.Portal>
            <Dialog.Content className="dialog-centered">
                <Dialog.Title className="text-white font-bold text-2xl">Collectionbrowser</Dialog.Title>
                <div className="flex gap-5 mb-5 mt-2">
                    <button className={`${!currentItem && '!hidden'} material-symbols-outlined text-white `} onClick={()=>{
                        if(parents.length === 1){
                            // Must be a collection
                            setCurrentItem(undefined)
                            setParents([])
                        } else if (currentItem) {
                            // Must be an item
                            const newParents = [...parents]
                            // Pop last item
                            newParents.pop()
                            setCurrentItem(newParents[newParents.length-1])
                            setParents(newParents)
                        }
                        else{
                            setCurrentItem(undefined)
                    }}}>arrow_back</button>
                    <div className="flex gap-2">
                        <span className="material-symbols-outlined text-white" onClick={()=>goToBack()}>house</span>
                        {parents.map((c,i)=><div key={i} className="text-white flex gap-2"><span className="material-symbols-outlined">chevron_right</span><span onClick={()=>goBackToElement(c)}>{c.name? c.name:c.info.name}</span></div>)}
                    </div>
                </div>
                <div className=" overflow-auto h-80">
                    {currentItem?currentItem.item!.map((c:any, index)=><CollectionItem key={index} collection={c}/>):
                        currentCollections.map((c,index)=>{
                        return <CollectionItem key={index} collection={c}/>
                    })}
                </div>
                {
                    currentItem&&
                    <button className="mt-5 text-center w-full rounded bg-green-700 flex justify-center pt-2 pb-2"
                            onClick={()=>createItem(parents[0] as CollectionDefinitionExtended, parents[parents.length-1])}>
                        <span className="material-symbols-outlined">upload</span>
                        Create new request</button>
                }

                <Dialog.Close  className="absolute top-0 right-0 p-2 text-white">
                <button className="IconButton" aria-label="Close" onClick={()=>setNewItemInserterOpen(false)}>
                    <span className="material-symbols-outlined">close</span>
                </button>
                </Dialog.Close>
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>
}
