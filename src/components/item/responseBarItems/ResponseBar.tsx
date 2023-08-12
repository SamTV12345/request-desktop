import {Editor} from "../Editor";
import {Tabs, TabsContent, TabsList, TabsTrigger} from "@radix-ui/react-tabs";

export const ResponseBar = ()=>{
    return <Tabs defaultValue="body" className="selector text-white p-2 tab-container">
        <TabsList className="query-param-list">
            <TabsTrigger value="body" className="">Body</TabsTrigger>
            <TabsTrigger value="cookies">Cookies</TabsTrigger>
            <TabsTrigger value="headers">Headers</TabsTrigger>
            <TabsTrigger value="test-results">Test Results</TabsTrigger>
        </TabsList>
        <TabsContent value="body" className={"response-body-viewer"}><Editor/></TabsContent>
        <TabsContent value="cookies">Change your password here.</TabsContent>
        <TabsContent value="headers"><div/></TabsContent>
        <TabsContent value="test-results"><div/></TabsContent>
    </Tabs>
}
