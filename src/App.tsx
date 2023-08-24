import {useEffect, useState} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import "./App.css";
import {SidebarComponent} from "./sidebar/SidebarComponent";
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "./store/store";
import {ContentModel} from "./components/item/ContentModel";
import {CollectionViewer} from "./components/collections/CollectionViewer";
import {getMetaData, setMetaData} from "./components/collection_authorization/TokenManagerService";
import { appWindow } from "@tauri-apps/api/window";
import { confirm } from '@tauri-apps/api/dialog';
const ContentModelDecider = () => {
    const currentCollection = useAPIStore(state => state.currentCollection)

    if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
        return <CollectionViewer/>
    } else {
        return <ContentModel/>
    }
}

appWindow.onCloseRequested(async e => {
    const metadata = [...useAPIStore.getState().metadata.values()]
    await setMetaData(metadata)
})
    .then(() => console.log("close requested"))
    .catch(e => console.log(e))

const App = () => {
    const setCollections = useAPIStore(state => state.setCollections)
    const metadata = useAPIStore(state => state.metadata)
    const [loading,setLoading] = useState<boolean>(true)
    async function get_collections(): Promise<CollectionDefinitionExtended[]> {
        // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
         return await invoke("get_collections")
    }

    const getMetadata = async () => {
        getMetaData()
            .then(c=>{
                c.forEach(c=>{
                    metadata.set(c.id,c)
                })
                setLoading(false)
            })
    }

    useEffect( () => {
        get_collections()
            .then(c => {
                setCollections(c)
            })
            .catch(e => console.log(e))
        getMetadata()
    }, [])

    if(loading){
        return <div className="loading-screen">
            <h1 className="loading-screen-heading">Loading...</h1>
        </div>
    }
    return (
        <>

            {/*<ResizableContext<HTMLDivElement> direction={"right"} initialSize={200} render={(size, targetRef) => (
                <div style={{background: "#AFA", width: size, height: "100px", position: "relative"}} ref={targetRef}>
                    <Resizer />
                </div>
            )} />

            <ResizableContext<HTMLDivElement> direction={"bottom"} initialSize={200} render={(size, targetRef) => (
                <div style={{background: "#AFA", width: "100px", height: size, position: "relative"}} ref={targetRef}>
                    <Resizer />
                </div>
            )} />

            <ResizableContext<HTMLDivElement> direction={"left"} initialSize={200} render={(size, targetRef) => (
                <div style={{background: "#AFA", width: size, height: "100px", position: "relative", float: "right"}} ref={targetRef}>
                    <Resizer />
                </div>
            )} />

            <ResizableContext<HTMLDivElement> direction={"top"} initialSize={200} render={(size, targetRef) => (
                <div style={{background: "#AFA", width: "100px", height: size, position: "fixed", bottom: "0"}} ref={targetRef}>
                    <Resizer />
                </div>
            )} />*/}

            {/*<ResizableBox direction={"top"} initialSize={100} style={{height: "100px", background: "#AFA", minWidth: "100px"}}/>*/}

            <SidebarComponent/>
            <div className="main-panel">
                <ContentModelDecider/>
            </div>
        </>
    )
}

export default App;
