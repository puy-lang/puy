program OverloadProcNames;

// A routine or method name without parentheses is called when an overload takes its result
// (semantics.md 4.7; Delphi passes a method name as a value instead, docs/delphi-diff.md).
// Put in a typed variable, the value is passed without being called. A variable that no
// candidate takes as a value is called (Delphi reports no matching overload).

type
  TF0 = reference to function: string;
  TFn = reference to function(X: Integer): string;

  TObj = class
    function GetS: string;
  end;

function TObj.GetS: string;
begin
  Result := 'method';
end;

function GetS: string;
begin
  Result := 'routine';
end;

procedure Z(const S: string); overload;
begin
  WriteLn('Z string ', S);
end;

procedure Z(F: TF0); overload;
begin
  WriteLn('Z TF0 ', F());
end;

procedure P(const S: string); overload;
begin
  WriteLn('P string ', S);
end;

procedure P(F: TFn); overload;
begin
  WriteLn('P TFn ', F(1));
end;

var
  O: TObj;
  F: TF0;
begin
  O := TObj.Create;
  Z(GetS);
  Z(O.GetS);
  F := GetS;
  Z(F);
  F := O.GetS;
  Z(F);
  P(F);
  O.Free;
end.
