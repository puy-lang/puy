program ProcResultMemberCall;

// A procedural value before `.` is called when its result has the member (objects.md 9.4, 9.11):
// `F.GetName`, `R.F.GetName` and `(F).GetName` call `F` and read the result's `GetName`, in an
// expression and as a statement.

type
  TObj = class
    function GetName: string;
  end;
  TObjFn = function: TObj;
  TRec = record
    F: TObjFn;
  end;
function TObj.GetName: string;
begin
  Result := 'obj';
end;
var
  Obj: TObj;
function MakeObj: TObj;
begin
  Result := Obj;
end;
var
  F: TObjFn;
  R: TRec;
  S: string;
begin
  Obj := TObj.Create;
  F := MakeObj;
  R.F := MakeObj;
  S := F.GetName;
  WriteLn(S);
  WriteLn(F.GetName);
  WriteLn(R.F.GetName);
  WriteLn((F).GetName);
  F.GetName;
  Obj.Free;
end.
