// ===== IfThen (docs/spec/stdlib/System.SysUtils/IfThen.md) =====

function IfThen(AValue: Boolean; const ATrue: Integer; const AFalse: Integer): Integer;
begin
  if AValue then Result := ATrue else Result := AFalse;
end;

function IfThen(AValue: Boolean; const ATrue: Int64; const AFalse: Int64): Int64;
begin
  if AValue then Result := ATrue else Result := AFalse;
end;

function IfThen(AValue: Boolean; const ATrue: Double; const AFalse: Double): Double;
begin
  if AValue then Result := ATrue else Result := AFalse;
end;

function IfThen(AValue: Boolean; const ATrue: string; const AFalse: string): string;
begin
  if AValue then Result := ATrue else Result := AFalse;
end;
