import {useEffect, useState} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import "./App.css";
import {SidebarComponent} from "./sidebar/SidebarComponent";
import {CollectionDefinitionExtended, useAPIStore} from "./store/store";
import {
    getMetaData, ITEM_TO_DB, META_DB,
    setMetaData,
    TOKEN_DB,
    TOKEN_DB_VERSION
} from "./components/collection_authorization/TokenManagerService";
import { appWindow } from "@tauri-apps/api/window";
import {
    Outlet
} from "react-router-dom";
import {MetaData} from "./models/MetaData";
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


    useEffect(() => {
        // DB migration
        const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
        dbRq.onupgradeneeded = (event) => {
            const db = (event.target as any).result
            db.createObjectStore(TOKEN_DB, {
                autoIncrement: true
            })
            db.createObjectStore(ITEM_TO_DB, {
                autoIncrement: true
            })
        }
        const dbRq2 = indexedDB.open(META_DB, TOKEN_DB_VERSION)
        dbRq2.onupgradeneeded = (event)=>{
            const db = dbRq.result
            db.createObjectStore(META_DB, {keyPath: "id", autoIncrement: true})
        }
    }, []);

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

    useEffect(()=>{
        if(!loading) {
            invoke('close_splashscreen')
        }
    }, [loading])

    if(loading){
        return <div></div>
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
               <Outlet/>
            </div>
        </>
    )
}


export default App;
