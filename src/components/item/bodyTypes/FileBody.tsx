import {CollectionDefinitionExtended, ItemDefinitionExtended, useAPIStore} from "../../../store/store";
import {open} from '@tauri-apps/api/dialog'
import {replaceItem} from "../../../utils/CollectionReplaceUtils";
export const FileBody = () => {
    const currentItem = useAPIStore(state => state.currentItem) as ItemDefinitionExtended
    const updateItem = useAPIStore(state => state.setCurrentItem)
    const currentCollection = useAPIStore(state => state.currentCollection) as CollectionDefinitionExtended
    const updateCollection = useAPIStore(state => state.setCurrentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const uploadFile = async () => {
        let fileProp:{
            src: string
        }
        if(typeof currentItem.request?.body?.file === "string"){
            fileProp = {
                src: typeof currentItem.request?.body?.file
            }
        } else if (typeof currentItem.request?.body?.file === "object"){
            fileProp = currentItem.request?.body?.file
        }


        await open({
            multiple: false,
            directory: false
        })
            .then(v=>{
                const updatedItem = {
                    ...currentItem,
                    request: {
                        ...currentItem.request!,
                        body: {
                            ...currentItem.request?.body!,
                            file:{
                                ...fileProp,
                                src: v as string
                            }
                        }
                    }
                }
                updateItem(updatedItem)
                const updatedCollection = replaceItem(currentCollection, updatedItem) as CollectionDefinitionExtended
                console.log(updatedCollection)
                updateCollection(updatedCollection)
                saveCollection()
            })
    }

    const resetFile = () => {
        updateItem({
            ...currentItem,
            request: {
                ...currentItem.request!,
                body: {
                    ...currentItem.request?.body!,
                    file: undefined
                }
            }
        })
    }

    const getFilename = (path: unknown) => {
        if (typeof path === 'object' && path && "src" in path && typeof path.src === 'string'){
            return path!.src.split("\\").pop()!.split("/").pop()
        }
        else{
            const path_typed = path as {
                src: string
            }
            return path_typed.src.split("\\").pop()!.split("/").pop()!
        }
    }


    return <>
        {
           currentItem.request?.body?.file == undefined?
               <button className="rounded-full bg-basecol p-2 m-5 pl-3 pr-3" onClick={()=>{
                   uploadFile()
               }}>Upload file</button>:
               <div className="relative w-52 m-5">
                <div className="rounded-full bg-basecol p-2 pr-10">{getFilename(currentItem.request.body.file)}</div>
               <button className="material-symbols-outlined absolute right-2 top-2" onClick={resetFile}>close</button>
               </div>
        }
    </>
}
