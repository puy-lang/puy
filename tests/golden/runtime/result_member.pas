program ResultMember;

// A member declared as `&Result` is found only after `.` (semantics.md 4.1): in a method of
// its type, `Result` alone is the function's result and `Self.Result` is the member.
// `System.RegularExpressions` declares `TMatch.Result` this way.

uses System.RegularExpressions;

type
  TCounter = class
  private
    FCount: Integer;
  public
    property &Result: Integer read FCount write FCount;
    function Next: Integer;
  end;

function TCounter.Next: Integer;
begin
  Self.Result := Self.Result + 1;
  Result := Self.Result * 10;
end;

var
  C: TCounter;
  M: TMatch;
begin
  C := TCounter.Create;
  WriteLn(C.Next, ' ', C.Next, ' ', C.Result);
  C.Free;
  M := TRegEx.Match('key=value', '(\w+)=(\w+)');
  WriteLn(M.Result('$2:$1'));
end.
