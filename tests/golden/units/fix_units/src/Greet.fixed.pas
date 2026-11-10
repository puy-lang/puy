unit Greet;

interface

procedure Hello;

implementation

procedure Hello;
var
  S: string = '';
begin
  S := S + 'Hello';
  WriteLn(S);
end;

end.
