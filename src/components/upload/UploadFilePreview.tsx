import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {VERSIONS} from "../../constants";
import {invoke} from "@tauri-apps/api/tauri";
import {Collection} from "postman-collection";


export const UploadFilePreview = ()=>{
    const collectionToUpload = useAPIStore(state=>state.fileToUpload)
    const setFileToUpload = useAPIStore(state=>state.setFileToUpload)
    const setCollections = useAPIStore(state=>state.setCollections)
    const collections = useAPIStore(state=>state.collections)
    if (collectionToUpload === undefined) return <div></div>

    const importCollection = ()=>{
        console.log(collectionToUpload.spec)
        invoke<CollectionDefinitionExtended>("insert_collection", {collection: collectionToUpload.spec})
            .then((c)=>{
                setCollections([...collections, c])
            })
            .catch(e=>console.log(e))
    }
    return <div>
        <h2 className="import-heading">The following file will be uploaded</h2>
        <table className="import-table">
            <thead>
            <tr>
                <th scope="col">Name</th>
                <th scope="col">Format</th>
                <th scope="col">Import as</th>
            </tr>
            </thead>
            <tbody>
            <tr>
                <td>{collectionToUpload.spec.info?.name}</td>
                {/* @ts-ignore */}
                <td>{VERSIONS.get(collectionToUpload.spec.info?.schema)}</td>
                <td>Collection</td>
            </tr>
            </tbody>
        </table>
        <div className="button-bar">
            <button className="upload-cancel-button" onClick={()=>{setFileToUpload(undefined)}}>Cancel</button>
            <button className="upload-preview-button" onClick={importCollection}>Import</button>
        </div>
    </div>
}
