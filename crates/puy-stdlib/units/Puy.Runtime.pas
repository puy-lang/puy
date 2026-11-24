unit Puy.Runtime;

interface

type
  TMemoryTypeUsage = record
    TypeName: string;
    Count: Int64;
    Bytes: Int64;
  end;

  TMemory = record
  private
    class function GetUsed: Int64; static;
    class function GetPeak: Int64; static;
    class function GetLimit: Int64; static;
    class function GetAvailable: Int64; static;
    class function GetIsolateUsed: Int64; static;
    class function GetIsolatePeak: Int64; static;
    class function GetIsolateLimit: Int64; static;
  public
    class property Used: Int64 read GetUsed;
    class property Peak: Int64 read GetPeak;
    class property Limit: Int64 read GetLimit;
    class property Available: Int64 read GetAvailable;
    class property IsolateUsed: Int64 read GetIsolateUsed;
    class property IsolatePeak: Int64 read GetIsolatePeak;
    class property IsolateLimit: Int64 read GetIsolateLimit;
    class procedure ResetIsolatePeak; static;
    class function LiveTypes: TArray<TMemoryTypeUsage>; static;
  end;

implementation

uses Puy.Runtime.Native;

class function TMemory.GetUsed: Int64;
begin
  Result := ProcessUsed;
end;

class function TMemory.GetPeak: Int64;
begin
  Result := ProcessPeak;
end;

class function TMemory.GetLimit: Int64;
begin
  Result := ProcessLimit;
end;

class function TMemory.GetAvailable: Int64;
begin
  Result := ProcessAvailable;
end;

class function TMemory.GetIsolateUsed: Int64;
begin
  Result := Puy.Runtime.Native.IsolateUsed;
end;

class function TMemory.GetIsolatePeak: Int64;
begin
  Result := Puy.Runtime.Native.IsolatePeak;
end;

class function TMemory.GetIsolateLimit: Int64;
begin
  Result := Puy.Runtime.Native.IsolateLimit;
end;

class procedure TMemory.ResetIsolatePeak;
begin
  Puy.Runtime.Native.ResetIsolatePeak;
end;

class function TMemory.LiveTypes: TArray<TMemoryTypeUsage>;
begin
  var Usage := Puy.Runtime.Native.LiveTypes;
  SetLength(Result, Length(Usage));
  for var I := 0 to High(Usage) do
  begin
    Result[I].TypeName := Usage[I].TypeName;
    Result[I].Count := Usage[I].Count;
    Result[I].Bytes := Usage[I].Bytes;
  end;
end;

end.
