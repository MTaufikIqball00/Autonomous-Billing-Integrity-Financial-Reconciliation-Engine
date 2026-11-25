"use client";

import React from "react";
import { PaginationMeta, UserSession } from "@/lib/api";
import { OverviewModule } from "./modules/OverviewModule";
import { DocumentsModule } from "./modules/DocumentsModule";
import { VendorsModule } from "./modules/VendorsModule";
import { ContractsModule } from "./modules/ContractsModule";
import { PurchaseOrdersModule } from "./modules/PurchaseOrdersModule";
import { InvoicesModule } from "./modules/InvoicesModule";
import { PaymentsModule } from "./modules/PaymentsModule";
import { AnomaliesModule } from "./modules/AnomaliesModule";
import { FindingsModule } from "./modules/FindingsModule";
import { ReviewQueueModule } from "./modules/ReviewQueueModule";
import { ReportsModule } from "./modules/ReportsModule";
import { ModelsModule } from "./modules/ModelsModule";
import { AgentsModule } from "./modules/AgentsModule";
import { SettingsAdminModule } from "./modules/SettingsAdminModule";

export interface FindingsFilters {
  category: string;
  severity: string;
  status: string;
  classification_type: string;
}

interface ModuleProps {
  activeTab: string;
  user: UserSession;
  overview: any;
  documents: any[];
  documentsPagination?: PaginationMeta;
  vendors: any[];
  vendorsPagination?: PaginationMeta;
  contracts: any[];
  contractsPagination?: PaginationMeta;
  purchaseOrders: any[];
  purchaseOrdersPagination?: PaginationMeta;
  invoices: any[];
  invoicesPagination?: PaginationMeta;
  payments: any[];
  paymentsPagination?: PaginationMeta;
  findings: any[];
  findingsPagination?: PaginationMeta;
  findingsFilters: FindingsFilters;
  onFindingsFilterChange: (next: FindingsFilters) => void;
  reviewQueue: any;
  agentRuns: any[];
  agentRunsPagination?: PaginationMeta;
  reports: any[];
  reportsPagination?: PaginationMeta;
  models: any[];
  externalAiEnv: boolean;
  settings: any[];
  users: any[];
  auditLogs: any[];
  auditPagination?: PaginationMeta;
  onPageChange: (tabKey: string, offset: number) => void;
  onRefresh: () => Promise<void>;
  onPasswordRotated: () => void;
  onOpenDocument: (docId: string) => void;
  onOpenInvoice: (invId: string) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}

export function DashboardModules(props: ModuleProps) {
  const {
    activeTab,
    user,
    overview,
    documents,
    documentsPagination,
    vendors,
    vendorsPagination,
    contracts,
    contractsPagination,
    purchaseOrders,
    purchaseOrdersPagination,
    invoices,
    invoicesPagination,
    payments,
    paymentsPagination,
    findings,
    findingsPagination,
    findingsFilters,
    onFindingsFilterChange,
    reviewQueue,
    agentRuns,
    agentRunsPagination,
    reports,
    reportsPagination,
    models,
    externalAiEnv,
    settings,
    users,
    auditLogs,
    auditPagination,
    onPageChange,
    onRefresh,
    onPasswordRotated,
    onOpenDocument,
    onOpenInvoice,
    setBanner,
  } = props;

  const canReview = user.role === "admin" || user.role === "reviewer";
  const canWrite = canReview || user.role === "analyst";
  const isAdmin = user.role === "admin";

  switch (activeTab) {
    case "overview":
      return (
        <OverviewModule
          user={user}
          overview={overview}
          findings={findings}
          onRefresh={onRefresh}
          setBanner={setBanner}
        />
      );
    case "documents":
      return (
        <DocumentsModule
          documents={documents}
          pagination={documentsPagination}
          vendors={vendors}
          canWrite={canWrite}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("documents", offset)}
          onOpenDocument={onOpenDocument}
          setBanner={setBanner}
        />
      );
    case "vendors":
      return (
        <VendorsModule
          vendors={vendors}
          pagination={vendorsPagination}
          canWrite={canWrite}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("vendors", offset)}
          setBanner={setBanner}
        />
      );
    case "contracts":
      return (
        <ContractsModule
          contracts={contracts}
          pagination={contractsPagination}
          vendors={vendors}
          canWrite={canWrite}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("contracts", offset)}
          setBanner={setBanner}
        />
      );
    case "purchase_orders":
      return (
        <PurchaseOrdersModule
          purchaseOrders={purchaseOrders}
          pagination={purchaseOrdersPagination}
          vendors={vendors}
          contracts={contracts}
          canWrite={canWrite}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("purchase_orders", offset)}
          setBanner={setBanner}
        />
      );
    case "invoices":
      return (
        <InvoicesModule
          invoices={invoices}
          pagination={invoicesPagination}
          vendors={vendors}
          canWrite={canWrite}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("invoices", offset)}
          onOpenInvoice={onOpenInvoice}
          setBanner={setBanner}
        />
      );
    case "payments":
      return (
        <PaymentsModule
          payments={payments}
          pagination={paymentsPagination}
          invoices={invoices}
          vendors={vendors}
          canWrite={canWrite}
          canReview={canReview}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("payments", offset)}
          setBanner={setBanner}
        />
      );
    case "agents":
      return (
        <AgentsModule
          agentRuns={agentRuns}
          pagination={agentRunsPagination}
          onPageChange={(offset) => onPageChange("agents", offset)}
          onRefresh={onRefresh}
        />
      );
    case "anomalies":
      return (
        <AnomaliesModule
          findings={findings}
          documents={documents}
          onOpenDocument={onOpenDocument}
        />
      );
    case "findings":
      return (
        <FindingsModule
          findings={findings}
          pagination={findingsPagination}
          filters={findingsFilters}
          onFilterChange={onFindingsFilterChange}
          onPageChange={(offset) => onPageChange("findings", offset)}
          onOpenDocument={onOpenDocument}
          setBanner={setBanner}
        />
      );
    case "review_queue":
      return (
        <ReviewQueueModule
          user={user}
          reviewQueue={reviewQueue}
          onRefresh={onRefresh}
          onOpenDocument={onOpenDocument}
          setBanner={setBanner}
        />
      );
    case "reports":
      return (
        <ReportsModule
          reports={reports}
          pagination={reportsPagination}
          onRefresh={onRefresh}
          onPageChange={(offset) => onPageChange("reports", offset)}
          setBanner={setBanner}
        />
      );
    case "models":
      return (
        <ModelsModule
          models={models}
          externalAiEnv={externalAiEnv}
          isAdmin={isAdmin}
          onRefresh={onRefresh}
          setBanner={setBanner}
        />
      );
    case "settings":
      return (
        <SettingsAdminModule
          user={user}
          settings={settings}
          users={users}
          auditLogs={auditLogs}
          auditPagination={auditPagination}
          onAuditPageChange={(offset) => onPageChange("settings", offset)}
          onRefresh={onRefresh}
          onPasswordRotated={onPasswordRotated}
          setBanner={setBanner}
        />
      );
    default:
      return null;
  }
}
