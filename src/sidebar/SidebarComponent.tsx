import {Box, Button, Stack} from "@mui/material";
import "../App.css";
import {useAPIStore} from "../store/store";
import {SidebarAccordeon} from "../components/SidebarAccordeon";
import * as Dialog from '@radix-ui/react-dialog';
import {Input} from "../components/Input";
import {useState} from "react";
import {Collection, CollectionDefinition} from "postman-collection";

export const SidebarComponent = ()=>{
    const collections = useAPIStore(state=>state.collections)
    const [isFilePicked, setIsFilePicked] = useState(false);
    const [collectionToUpload, setCollectionToUpload] = useState<CollectionDefinition|undefined>()
    const changeHandler = (event:any) => {
        const file = event.target.files[0];
        const reader = new FileReader();
        reader.onload = function(event) {
            // The file's text will be printed here
            const result: CollectionDefinition = JSON.parse(event.target!.result as string)
            setCollectionToUpload(result)
        };
        reader.readAsText(file);
    };


    return (
            <div className="sidebar">

                <div className="sidebar-import-buttons">
                    <button>New</button>
                    <Dialog.Root>
                        <Dialog.Trigger>
                            <button onClick={()=>{}}>Import</button>
                        </Dialog.Trigger>
                        <Dialog.Portal className="">
                            <Dialog.Content className="dialog-centered">
                                {!collectionToUpload&&<Input type="file" onChange={(e)=>changeHandler(e)}/>}
                                {collectionToUpload&&<div>
                                    {
                                        collectionToUpload&& <div>CollectionToUpload:
                                            <div>{collectionToUpload.info?.name}</div>
                                            <div>{collectionToUpload.info?.schema}</div>
                                            </div>

                                    }
                                </div>}
                            </Dialog.Content>
                        </Dialog.Portal>
                    </Dialog.Root>
                </div>
                {
                    collections.map((c, i)=>{
                        return (
                            <Box sx={{padding: '20px'}}>
                                <Stack key={i} direction="row" spacing={2}>
                                    <SidebarAccordeon key={c.id} collection={c}/>
                                </Stack>
                            </Box>
                        )
                    })
                }

            </div>
    )
}
