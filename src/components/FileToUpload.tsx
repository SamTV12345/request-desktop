import React from 'react'
import {useDropzone} from 'react-dropzone'
import {useAPIStore} from "../store/store";

export const FileToUpload = ()=>{
    const setFileUploadString = useAPIStore(state=>state.setFileToUpload)


    const onDrop = ([file])=>{
        const reader = new FileReader();
        reader.onload = function(e) {
            const contents = e.target!.result;
            setFileUploadString({
                spec: JSON.parse(contents as string),
                text: contents as string
            })
        };
        reader.readAsText(file);
    }

    const {getRootProps, getInputProps, isDragActive} = useDropzone({onDrop})


    return (
        <div {...getRootProps()} className="dropzone">
            <input {...getInputProps()} />
            {
                isDragActive ?
                    <p>Drop the files here ...</p> :
                    <p>Drag 'n' drop some files here, or click to select files</p>
            }
        </div>
    )
}
