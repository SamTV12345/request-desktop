import {Tabs, TabsContent, TabsList, TabsTrigger} from "../bareComponents/Tabs";
import {NavLink, Outlet, useLocation, useNavigate} from "react-router-dom";
import {useEffect, useState} from "react";

export const QueryParam = () => {
    const navigate = useNavigate()
    const [param, setParam] = useState<string>("params")


    return <Tabs value={param} onValueChange={v=>{
        setParam(v)
        navigate(v)
    }} className="request-details-section selector text-white">
        <TabsList className="query-param-list">
            <TabsTrigger value="params" className=""><NavLink to="params">Params</NavLink></TabsTrigger>
            <TabsTrigger value="authorization"><NavLink to="authorization">Authorization</NavLink></TabsTrigger>
            <TabsTrigger value="headers"><NavLink to="headers">Headers</NavLink></TabsTrigger>
            <TabsTrigger value="body" ><NavLink to="body">Body</NavLink></TabsTrigger>
            <TabsTrigger value="prerequest"><NavLink to="prerequest">Prerequest</NavLink></TabsTrigger>
            <TabsTrigger value="tests"><NavLink to="tests">Tests</NavLink></TabsTrigger>
            <TabsTrigger value="settings"><NavLink to="settings"></NavLink></TabsTrigger>
        </TabsList>
            <Outlet/>
    </Tabs>
}
