// A built-in exception the VM raises carries its class's property in the JSON report
// (`EConvertError.Value`; docs/spec/exceptions.md 2, 7.1).
uses System.SysUtils;

WriteLn(StrToInt('12'));
WriteLn(StrToInt('x1'));
