import {useAPIStore} from "../../store/store";
import {VERSIONS} from "../../constants";
import {invoke} from "@tauri-apps/api/tauri";


export const UploadFilePreview = ()=>{
    const collectionToUpload = useAPIStore(state=>state.fileToUpload)
    const setFileToUpload = useAPIStore(state=>state.setFileToUpload)

    if (collectionToUpload === undefined) return <div></div>

    const importCollection = ()=>{
        invoke("insert_collection", {collection: collectionToUpload.spec})
            .then((c)=>{
                console.log(c)
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
