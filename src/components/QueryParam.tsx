import {Tabs} from './Tabs'
import {TabsContent, TabsList, TabsTrigger} from "./Tabs";
import DataTable from "./QueryParamTable";
export const QueryParam = () => {
    return <Tabs defaultValue="params" className="w-[400px]">
        <TabsList className="query-param-list">
            <TabsTrigger value="params">Params</TabsTrigger>
            <TabsTrigger value="authorization">Authorization</TabsTrigger>
            <TabsTrigger value="headers">Headers</TabsTrigger>
            <TabsTrigger value="body">Body</TabsTrigger>
            <TabsTrigger value="prerequest">Pre-request Script</TabsTrigger>
            <TabsTrigger value="tests">Tests</TabsTrigger>
            <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>
        <TabsContent value="params"><DataTable/></TabsContent>
        <TabsContent value="authorization">Change your password here.</TabsContent>
        <TabsContent value="headers">Change your password here.</TabsContent>
        <TabsContent value="body">Change your password here.</TabsContent>
        <TabsContent value="prerequest">Pre-request Script</TabsContent>
        <TabsContent value="tests">Change your password here.</TabsContent>
        <TabsContent value="settings">Change your password here.</TabsContent>
    </Tabs>
}
