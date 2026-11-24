// ===== TBooleanHelper (docs/spec/stdlib/System/Helpers.md) =====

class function TBooleanHelper.Parse(const S: string): Boolean;
begin
  Result := StrToBool(S);
end;

class function TBooleanHelper.TryParse(const S: string; out Value: Boolean): Boolean;
begin
  Value := False;
  try
    Value := StrToBool(S);
  except
    on EConvertError do
      Exit(False);
  end;
  Result := True;
end;

function TBooleanHelper.ToInteger: Integer;
begin
  Result := Ord(Self);
end;
