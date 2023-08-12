import {TabsContent, TabsList, TabsTrigger, Tabs} from "../bareComponents/Tabs";
import {ParamTable} from "./QueryParamTable";
import {HeaderDataTable} from "./HeaderDataTable";
import {HeaderTable} from "./HeaderTable";

export const QueryParam = () => {
    return <Tabs defaultValue="params" className="request-details-section">
        <TabsList className="query-param-list">
            <TabsTrigger value="params" className="">Params</TabsTrigger>
            <TabsTrigger value="authorization">Authorization</TabsTrigger>
            <TabsTrigger value="headers">Headers</TabsTrigger>
            <TabsTrigger value="body">Body</TabsTrigger>
            <TabsTrigger value="prerequest">Pre-request Script</TabsTrigger>
            <TabsTrigger value="tests">Tests</TabsTrigger>
            <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>
        <TabsContent value="params"><ParamTable/></TabsContent>
        <TabsContent value="authorization">Change your password here.</TabsContent>
        <TabsContent value="headers"><HeaderTable/></TabsContent>
        <TabsContent value="body">Change your password here.</TabsContent>
        <TabsContent value="prerequest">Pre-request Script</TabsContent>
        <TabsContent value="tests">Change your password here.</TabsContent>
        <TabsContent value="settings">Change your password here.</TabsContent>
    </Tabs>
}
