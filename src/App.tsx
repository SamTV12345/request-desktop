import {useEffect} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import "./App.css";
import {SidebarComponent} from "./sidebar/SidebarComponent";
import {DisplayType, useAPIStore} from "./store/store";
import {Collection} from "postman-collection";
import {ContentModel} from "./components/item/ContentModel";
import {CollectionViewer} from "./components/collections/CollectionViewer";

const ContentModelDecider = () => {
    const currentCollection = useAPIStore(state => state.currentCollection)

    if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
        return <CollectionViewer/>
    } else {
        return <ContentModel/>
    }
}


const App = () => {
    const setCollections = useAPIStore(state => state.setCollections)
    const currentCollection = useAPIStore(state => state.currentItem)

    async function get_collections(): Promise<Collection[]> {
        // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
         return await invoke("get_collections")
    }

    useEffect(() => {
        get_collections()
            .then(c => {
                setCollections(c)
            })
            .catch(e => console.log(e))
    }, [])

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
