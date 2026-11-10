unit MyApp.Models.User;

interface

uses Greeting, System.SysUtils;

function Describe(const Name: string): string;

implementation

function Describe(const Name: string): string;
begin
  Result := Greet(Name) + ' (' + IntToStr(Count) + ')';
end;

end.
