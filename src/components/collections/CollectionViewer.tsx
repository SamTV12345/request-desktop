import {Tabs, TabsContent, TabsList, TabsTrigger} from "../bareComponents/Tabs";
import {VariableViewer} from "./VariableViewer";
import {CollectionAuthorization} from "./CollectionAuthorization";
import {NavLink, Outlet, useNavigate} from "react-router-dom";
import {useEffect, useState} from "react";
import {SidebarComponent} from "../../sidebar/SidebarComponent";

export const CollectionViewer = ()=>{
    const navigate = useNavigate()
    const [params,setParams] = useState<string>('authorization')
    return (
        <><SidebarComponent/>
            <div className="main-panel">
                <Tabs value={params} onValueChange={v => {
                    setParams(v);
                    navigate(v);
                }} className="selector text-white p-2">
                    <TabsList className="query-param-list">
                        <TabsTrigger value="authorization"><NavLink
                            to="authorization">Authorisierung</NavLink></TabsTrigger>
                        <TabsTrigger value="pre-request-script"><NavLink to="pre-request-script">Pre-request
                            Script</NavLink></TabsTrigger>
                        <TabsTrigger value="tests"><NavLink to="tests">Tests</NavLink></TabsTrigger>
                        <TabsTrigger value="variables"><NavLink to="variables">Variablen</NavLink></TabsTrigger>
                    </TabsList>
                    <Outlet/>
                </Tabs>
            </div>
        </>
    )
}

/*


        <TabsContent value="authorization"><CollectionAuthorization/></TabsContent>
        <TabsContent value="pre-request-script">Change your password here.</TabsContent>
        <TabsContent value="tests"><div/></TabsContent>
        <TabsContent value="variables"><VariableViewer/></TabsContent>
 */
