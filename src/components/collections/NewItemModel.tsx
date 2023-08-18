import * as Dialog from "@radix-ui/react-dialog";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {Collection, CollectionDefinition} from "postman-collection";
import {invoke} from "@tauri-apps/api/tauri";
import {NewItemInserter} from "./NewItemInserter";

export const NewItemModel = ()=>{
    const openNewCollectionModel = useAPIStore(state=>state.openNewCollectionModal)
    const setOpenNewCollectionModel = useAPIStore(state=>state.setOpenNewCollectionModal)
    const setInsertNewCollection = useAPIStore(state=>state.setNewItemInserterOpen)
    const insertNewCollection = useAPIStore(state=>state.insertNewCollection)

    return                         <Dialog.Root open={openNewCollectionModel}>
        <NewItemInserter/>
        <Dialog.Portal className="">
            <Dialog.Content className="dialog-centered">
                <Dialog.Title className="text-white font-bold text-2xl">Create new</Dialog.Title>
                <Dialog.Close  className="absolute top-0 right-0 p-2 text-white">

                    <button className="IconButton" aria-label="Close">
                        <span className="material-symbols-outlined" onClick={()=>setOpenNewCollectionModel(false)}>Close</span>
                    </button>
                </Dialog.Close>
                    <h2 className="text-stone-400">Building Blocks</h2>
                <div className="grid gap-5 grid-cols-2">
                    <button className="gap-5 flex text-white" onClick={()=>{
                        setInsertNewCollection(true)
                    }}>
                        <span className="material-symbols-outlined rotate-45 text-lime-50 scale-95">sync_alt</span>
                        <span className="">HTTP Request</span>
                    </button>
                    <button className="gap-5 flex text-white" onClick={()=>{
                        // @ts-ignore
                        const defaultCollection:Collection = {info: {name: "New Collection",
                                schema:"https://schema.getpostman.com/json/collection/v2.1.0/collection.json"}, item: []}

                        invoke<CollectionDefinitionExtended>('insert_collection',{
                            collection:defaultCollection
                    }).then((c)=>{
                            insertNewCollection(c)
                        })
                    }}>
                        <span className="material-symbols-outlined">filter_none</span>
                        <span>Collections</span>
                    </button>
                </div>
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>
}
