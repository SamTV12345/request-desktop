import "../App.css";
import {useAPIStore} from "../store/store";
import {SidebarAccordeon} from "../components/bareComponents/SidebarAccordeon";
import * as Dialog from '@radix-ui/react-dialog';
import {Tabs, TabsContent, TabsList, TabsTrigger} from "../components/bareComponents/Tabs";
import {FileToUpload} from "../components/upload/FileToUpload";
import {UploadFilePreview} from "../components/upload/UploadFilePreview";

export const SidebarComponent = ()=>{
    const collections = useAPIStore(state=>state.collections)
    const collectionToUpload = useAPIStore(state=>state.fileToUpload)


    return (
            <div className="sidebar">

                <div className="sidebar-import-buttons">
                    <button>New</button>
                    <Dialog.Root>
                        <Dialog.Trigger>
                            <button className="sidebar-button"  onClick={()=>{}}>Import</button>
                        </Dialog.Trigger>
                        <Dialog.Portal className="">
                            <Dialog.Content className="dialog-centered">
                                <h2 className="import-heading">Import collection</h2>
                                <Tabs defaultValue="file" className="import-tabs">
                                    <TabsList className="query-param-list">
                                        <TabsTrigger value="file">File</TabsTrigger>
                                        <TabsTrigger value="folder">Folder</TabsTrigger>
                                        <TabsTrigger value="link">Link</TabsTrigger>
                                        <TabsTrigger value="raw-text">Raw text</TabsTrigger>
                                        <TabsTrigger value="code-repository">Code repository</TabsTrigger>
                                    </TabsList>
                                    <TabsContent value="file">
                                        {
                                            !collectionToUpload&&
                                                <FileToUpload/>
                                        }
                                        {collectionToUpload&&<div>
                                            {
                                                collectionToUpload&& <UploadFilePreview/>

                                            }
                                        </div>}</TabsContent>
                                    <TabsContent value="folder">Folder</TabsContent>
                                    <TabsContent value="link">Link</TabsContent>
                                    <TabsContent value="raw-text">Raw text</TabsContent>
                                    <TabsContent value="code-repository">Code repository </TabsContent>
                                </Tabs>

                            </Dialog.Content>
                        </Dialog.Portal>
                    </Dialog.Root>
                </div>
                <div className="sidebar-collection-list">
                {
                    collections.map((c, i)=>{
                        return (
                                    <SidebarAccordeon key={c.id} collection={c}/>
                        )
                    })
                }
                </div>
            </div>
    )
}
