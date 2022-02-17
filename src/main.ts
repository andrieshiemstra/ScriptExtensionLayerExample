import { calc } from 'ModuleA.ts';
console.log("7*8=%s", calc(7, 8));

type MyApp = EventTarget & {
    printSomething: (thing: string) => void
};

const myApp: MyApp = com.mycompany.MyApp;

com.mycompany.MyApp.addEventListener("request", (evt) => {
    myApp.printSomething("Just letting you know javascript received your event loud and clear!");
    console.log("logging from javascript");
});