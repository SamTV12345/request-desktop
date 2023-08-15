import * as Dialog from '@radix-ui/react-dialog'
import {useAPIStore} from "../../store/store";
import {useState} from "react";
import {Collection, CollectionDefinition, ItemDefinition} from "postman-collection";
export const NewItemInserter = () => {
    const newItemInserterOpen = useAPIStore(state => state.newItemInserterOpen)
    const setNewItemInserterOpen = useAPIStore(state => state.setNewItemInserterOpen)
    const currentCollections = useAPIStore(state => state.collections)
    const [currentItem,setCurrentItem] = useState<CollectionDefinition>()
    const [parents, setParents] = useState<any[]>([])

    const isItemGroupDefinition = (item: any): item is ItemDefinition => {
        return typeof item === "object" && 'item' in item && item.item != undefined
    }

    const isCollectionDefinition = (item: any): item is CollectionDefinition => {
        return typeof item === "object" && 'info' in item && item.item == undefined
    }
    const changeToSubDirectory = (collection: Collection|ItemDefinition)=>{
        // @ts-ignore
        setCurrentItem(collection)
        console.log(collection)
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

    return <Dialog.Root open={newItemInserterOpen}>
        <Dialog.Portal>
            <Dialog.Content className="dialog-centered">
                <Dialog.Title className="text-white font-bold text-2xl">Collectionbrowser</Dialog.Title>
                <div className="flex gap-5">
                    <button className={`${!currentItem && '!hidden'} material-symbols-outlined text-white `} onClick={()=>{
                        if(currentItem &&"info" in currentItem){
                            // Must be a collection
                            setCurrentItem(undefined)
                            setParents([])
                        }else if (currentItem) {
                            console.log("currentItem", currentItem)
                            // Must be an item
                            const newParents = [...parents]
                            const item = newParents.pop()
                            console.log("item", item.item)
                            setCurrentItem(newParents.pop())
                            setParents(newParents)
                        }
                        else{
                            console.log("currentItem2", currentItem)
                            setCurrentItem(undefined)
                    }}}>arrow_back</button>
                    <div className="flex gap-2">
                        {parents.map(c=><div className="text-white flex gap-2"><span className="material-symbols-outlined">chevron_right</span><span>{c.name? c.name:c.info.name}</span></div>)}
                    </div>
                </div>
                <div className=" overflow-auto h-80">
                    {currentItem?currentItem.item!.map((c:any)=><CollectionItem collection={c}/>):
                        currentCollections.map(c=>{
                        return <CollectionItem collection={c}/>
                    })}
                </div>
                {
                    currentItem&&
                    <button className="mt-5 text-center w-full rounded bg-green-700 flex justify-center pt-2 pb-2">
                        <span className="material-symbols-outlined ">upload</span>
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
