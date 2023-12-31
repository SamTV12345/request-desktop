import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import "./App.css"
import './font-faces.css'
import "material-symbols/outlined.css"
import {
    createBrowserRouter,
    createRoutesFromElements,
    Navigate,
    Route,
    RouterProvider,
    useNavigate
} from "react-router-dom";
import {CollectionViewer} from "./components/collections/CollectionViewer";
import {ContentModel} from "./components/item/ContentModel";
import {BodyDecider} from "./components/item/BodyDecider";
import {RawBody} from "./components/item/bodyTypes/RawBody";
import {UrlEncodedBody} from "./components/item/bodyTypes/UrlEncodedBody";
import {FormDataBody} from "./components/item/bodyTypes/FormDataBody";
import {FileBody} from "./components/item/bodyTypes/FileBody";
import {ParamTable} from "./components/item/QueryParamTable";
import {CollectionAuthorization} from "./components/collections/CollectionAuthorization";
import {HeaderTable} from "./components/item/HeaderTable";
import {Prerequest} from "./components/bareComponents/Prerequest";
import {Tests} from "./components/bareComponents/Tests";
import {Settings} from "./components/bareComponents/Settings";
import {ItemGroupViewer} from "./components/item/ItemGroupViewer";

const ErrorFallback = ({error}:any) => {
    const navigate = useNavigate()

    return <button onClick={()=>navigate("/")}>

    </button>
}

const router = createBrowserRouter(createRoutesFromElements(
    <>
        <Route path="/" element={<App />}>
            <Route path="collection" element={<CollectionViewer/>}>
                <Route index element={<Navigate to="authorization"/>}/>
                <Route path="authorization" element={<CollectionAuthorization/>}/>
                <Route path="pre-request-script" element={<Prerequest/>}/>
                <Route path="tests" element={<Tests/>}/>
                <Route path="settings" element={<Settings/>}/>
                <Route path="variables" element={<div/>}/>
            </Route>
            <Route path="item" element={<ContentModel/>}>
                <Route index  element={<Navigate to="params"/>}/>
                <Route path="params" element={<ParamTable/>} />
                <Route path="headers" element={<HeaderTable/>}/>
                <Route path="authorization" element={<CollectionAuthorization/>}/>
                <Route path="prerequest" element={<Prerequest/>}/>
                <Route path="tests" element={<Tests/>}/>
                <Route path="settings" element={<Settings/>}/>
                <Route path="body" element={<BodyDecider/>}>
                    <Route path="raw" element={<RawBody/>}/>
                    <Route path="urlencoded" element={<UrlEncodedBody/>}/>
                    <Route path="formdata" element={<FormDataBody/>}/>
                    <Route path="file" element={<FileBody/>}/>
                    <Route path="*" element={<div/>}/>
                </Route>
            </Route>
            <Route path="group" element={<ItemGroupViewer/>}  errorElement={<ErrorFallback/>}>
                <Route path="variables" element={<div/>}  errorElement={<ErrorFallback/>}/>
                <Route index element={<Navigate to="authorization"/>}  errorElement={<ErrorFallback/>}/>
                <Route path="authorization" element={<CollectionAuthorization/>}  errorElement={<ErrorFallback/>}/>
                <Route path="params" element={<ParamTable/>}  errorElement={<ErrorFallback/>}/>
                <Route path="pre-request-script" element={<Prerequest/>}  errorElement={<ErrorFallback/>}/>
                <Route path="headers" element={<HeaderTable/>}  errorElement={<ErrorFallback/>}/>
                <Route path="prerequest" element={<Prerequest/>}  errorElement={<ErrorFallback/>}/>
                <Route path="tests" element={<Tests/>}  errorElement={<ErrorFallback/>}/>
            </Route>
            <Route path="*" element={<Navigate to="/collection"/>} errorElement={<ErrorFallback/>}/>
        </Route>
    </>
))




ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
      <RouterProvider router={router}/>
  </React.StrictMode>
);
