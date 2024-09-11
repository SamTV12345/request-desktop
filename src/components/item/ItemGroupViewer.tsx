import {Tabs, TabsList, TabsTrigger} from "../bareComponents/Tabs";
import {NavLink, Outlet, useNavigate} from "react-router-dom";
import {useState} from "react";
import {SidebarComponent} from "../../sidebar/SidebarComponent";

export const ItemGroupViewer = ()=>{
    const [params,setParams] = useState<string>('authorization')
    const navigate = useNavigate()

    return <><SidebarComponent/>
        <div className="main-panel"><Tabs value={params} onValueChange={v=>{
        setParams(v)
        navigate(v)}} className="selector text-white p-2">
        <TabsList className="query-param-list">
            <TabsTrigger value="authorization"><NavLink to="authorization">Authorisierung</NavLink></TabsTrigger>
            <TabsTrigger value="pre-request-script"><NavLink to="pre-request-script">Pre-request Script</NavLink></TabsTrigger>
            <TabsTrigger value="tests"><NavLink to="tests">Tests</NavLink></TabsTrigger>
            <TabsTrigger value="variables"><NavLink to="variables">Variablen</NavLink></TabsTrigger>
        </TabsList>
        <Outlet/>
    </Tabs>
        </div>
        </>
}
