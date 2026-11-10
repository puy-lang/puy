unit Shapes;

interface

procedure Draw;
procedure Fill(Color: Integer);
function Area(W, H: Integer): Integer;
procedure Clear; forward;
procedure Quick; begin end;
procedure Twin(N: Integer); overload;
procedure Twin(S: string); overload;

const
  Size = 3;

implementation

const
  Size = 4;

procedure Fill(Color: string);
begin
end;

function Area(W, H: Integer): Integer;
begin
  Result := W * H;
end;

procedure Clear;
begin
end;

procedure Twin(N: Integer);
begin
end;

procedure Twin(S: string);
begin
end;

end.
