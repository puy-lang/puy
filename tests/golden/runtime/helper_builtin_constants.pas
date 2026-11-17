program HelperBuiltinConstants;

// The built-in constants and parameterless functions of `System` are values, so `.` on them
// looks up their type's helpers (objects.md 10.4) like a declared constant does: `True`,
// `False` and `MaxInt` take the built-in helpers (10.7), `Pi` and `ParamCount` are called
// without parentheses first, and a helper of the program applies to them too.

type
  TBoolText = record helper for Boolean
    function YesNo: string;
  end;

  TRealText = record helper for Double
    function Twice: Double;
  end;

function TBoolText.YesNo: string;
begin
  if Self then
    Result := 'yes'
  else
    Result := 'no';
end;

function TRealText.Twice: Double;
begin
  Result := Self * 2;
end;

const
  C = True;

var
  S: string;
begin
  WriteLn(True.ToString, ' ', False.ToString, ' ', C.ToString, ' ', (1 > 0).ToString);
  WriteLn(MaxInt.ToString, ' ', ParamCount.ToString);
  WriteLn(Pi.ToString);
  S := Pi.ToString + '!';
  WriteLn(S);
  WriteLn(True.YesNo, ' ', False.YesNo, ' ', Pi.Twice);
end.
