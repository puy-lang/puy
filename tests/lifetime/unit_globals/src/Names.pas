unit Names;

interface

uses Store;

var
  Greeting: string;

implementation

var
  Copy: array of string;

initialization
  Greeting := 'hello ' + Items[0];
  Copy := Items;
finalization
  WriteLn('fini Names ', Greeting, ' ', Length(Copy), ' ', Length(Items));
end.
